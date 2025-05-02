// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Repository store management.
//!
//! This module provides utilities to manipulate and manage OCD's repository store. Similar to the
//! cluster definition, the repository store houses a root repository with a set of node
//! repositories that are all initially defined through the cluster definition housed in the root
//! repository. The repository store always reflects the changes made to the cluster definition
//! such that a top-down heirarchy is followed, with the cluster definition at the top and
//! repository store at the bottom.

use crate::{
    fs::{config_dir, data_dir},
    model::{Cluster, DeploymentKind, DirAlias, NodeEntry},
    utils::{glob_match, syscall_interactive, syscall_non_interactive},
    Error, Result,
};

use auth_git2::{GitAuthenticator, Prompter};
use futures::{stream, StreamExt};
use git2::{
    build::RepoBuilder, Config, FetchOptions, RemoteCallbacks, Repository, RepositoryInitOptions,
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::{Password, Text};
use std::{
    ffi::OsString,
    fmt::Write as FmtWrite,
    fs::{remove_dir_all, File},
    io::Write as IoWrite,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tracing::{debug, info, instrument, warn};

pub struct Root {
    entry: RepoEntry,
    deployer: RepoEntryDeployer,
}

impl Root {
    pub fn new_clone(url: impl AsRef<str>) -> Result<Self> {
        let bar = ProgressBar::no_length();
        let entry = RepoEntry::builder("root")?
            .deployment(DeploymentKind::BareAlias(DirAlias::default()))
            .authentication_prompter(ProgressBarAuthenticator::new(ProgressBarKind::SingleBar(bar.clone())))
            .clone(url.as_ref(), &bar)?;
        bar.finish_and_clear();

        let deployer = RepoEntryDeployer::new(&entry);
        let mut root = Self { entry, deployer };
        let cluster = root.extract_cluster_file()?;

        root.entry.set_deployment(DeploymentKind::BareAlias(cluster.root.dir_alias.clone()));
        root.deployer.deploy_with(BareAliasDeployment, &root.entry, DeployAction::Deploy)?;

        Ok(root)
    }

    pub fn new_open() -> Result<Self> {
        let entry = RepoEntry::builder("root")?.open()?;
        let deployer = RepoEntryDeployer::new(&entry);
        let mut root = Self { entry, deployer };
        let cluster = root.extract_cluster_file()?;

        root.entry.set_deployment(DeploymentKind::BareAlias(cluster.root.dir_alias.clone()));
        root.deployer.deploy_with(RootDeployment, &root.entry, DeployAction::Deploy)?;

        Ok(root)
    }

    pub fn new_init() -> Result<Self> {
        let entry = RepoEntry::builder("root")?
            .deployment(DeploymentKind::BareAlias(DirAlias::default()))
            .init()?;
        let deployer = RepoEntryDeployer::new(&entry);

        Ok(Self { entry, deployer })
    }

    pub fn deploy(&self, action: DeployAction) -> Result<()> {
        self.deployer.deploy_with(RootDeployment, &self.entry, action)
    }

    pub fn current_branch(&self) -> Result<String> {
        self.entry.current_branch()
    }

    pub(crate) fn extract_cluster_file(&self) -> Result<Cluster> {
        if self.entry.is_empty()? {
            warn!("Root is empty, no cluster.toml file to extract");
            return Ok(Cluster::default());
        }

        let commit = self.entry.repository.head()?.peel_to_commit()?;
        let tree = commit.tree()?;
        let blob = if let Some(entry) = tree.get_name(".config/ocd/cluster.toml") {
            entry.to_object(&self.entry.repository)?.peel_to_blob()?
        } else {
            let entry = tree.get_name("cluster.toml").ok_or(Error::NoClusterFile)?;
            entry.to_object(&self.entry.repository)?.peel_to_blob()?
        };

        let cluster: Cluster = String::from_utf8_lossy(blob.content()).into_owned().parse()?;
        debug!("Extracted the following content from cluster.toml\n{cluster}");

        Ok(cluster)
    }

    #[instrument(skip(self), level = "debug")]
    pub fn nuke(&self) -> Result<()> {
        let cluster: Cluster = self.extract_cluster_file()?;
        self.deployer.deploy_with(BareAliasDeployment, &self.entry, DeployAction::Undeploy)?;

        for (name, node) in &cluster.nodes {
            if !data_dir()?.join(name).exists() {
                warn!("Node {name:?} not found in repository store");
                continue;
            }

            let repo = Node::new_open(name, node)?;
            repo.deploy(DeployAction::Undeploy)?;
        }

        remove_dir_all(config_dir()?)?;
        info!("Configuration directory removed");
        remove_dir_all(data_dir()?)?;
        info!("Data directory removed");

        Ok(())
    }

    pub fn gitcall(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<()> {
        self.entry.gitcall_interactive(args)
    }
}

/// Manage node repository in repository store.
pub struct Node {
    entry: RepoEntry,
    deployer: RepoEntryDeployer,
}

impl Node {
    /// Clone node repository into repository store.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Git2`] for any failure internal repository failure.
    /// - Return [`Error::Git2FileNotFound`] if cluster definition does not exist in root.
    /// - Return [`Error::SyscallInteractive`] for deployment failure.
    /// - Return [`Error::Io`] for failed writes to sparse checkout file.
    pub fn new_clone(name: impl AsRef<str>, node: &NodeEntry) -> Result<Self> {
        let bar = ProgressBar::no_length();
        let entry = RepoEntry::builder(name.as_ref())?
            .deployment(node.deployment.clone())
            .authentication_prompter(ProgressBarAuthenticator::new(ProgressBarKind::SingleBar(bar.clone())))
            .clone(&node.url, &bar)?;
        let mut deployer = RepoEntryDeployer::new(&entry);
        deployer.add_excluded(node.excluded.iter().flatten());

        Ok(Self { entry, deployer })
    }

    /// Initialize new node repository in repository store.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Git2`] if repository could not be initialized.
    #[instrument(skip(name, node))]
    pub fn new_init(name: impl AsRef<str>, node: &NodeEntry) -> Result<Self> {
        info!("Initialize node repository {:?}", name.as_ref());
        let entry = RepoEntry::builder(name.as_ref())?
            .deployment(node.deployment.clone())
            .init()?;
        let mut deployer = RepoEntryDeployer::new(&entry);
        deployer.add_excluded(node.excluded.iter().flatten());

        Ok(Self { entry, deployer })
    }

    /// Construct new node by opening existing node repository.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Git2`] if repository could not be opened.
    pub fn new_open(name: impl AsRef<str>, node: &NodeEntry) -> Result<Self> {
        let entry = RepoEntry::builder(name.as_ref())?
            .deployment(node.deployment.clone())
            .open()?;
        let mut deployer = RepoEntryDeployer::new(&entry);
        deployer.add_excluded(node.excluded.iter().flatten());

        Ok(Self { entry, deployer })
    }

    /// Path to node repository.
    pub fn path(&self) -> &Path {
        self.entry.repository.path()
    }

    /// Name of node repository.
    pub fn name(&self) -> &str {
        self.entry.name()
    }

    /// Get current name of branch.
    ///
    /// # Errors
    ///
    /// - Will fail if HEAD is not pointing to a named branch.
    pub fn current_branch(&self) -> Result<String> {
        self.entry.current_branch()
    }

    /// Deploy node repository.
    ///
    /// # Errors
    ///
    /// - Will fail if deployment action fails for whatever reason.
    pub fn deploy(&self, action: DeployAction) -> Result<()> {
        self.deployer.deploy_with(BareAliasDeployment, &self.entry, action)
    }

    /// Make interactive call to Git binary.
    ///
    /// # Errors
    ///
    /// - Will fail if Git binary cannot be found.
    /// - Will fail if provided arguments are invalid.
    pub fn gitcall(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<()> {
        self.entry.gitcall_interactive(args)
    }
}

/// Clone all nodes in cluster definition asynchronously.
pub struct MultiNodeClone {
    nodes: Vec<GitBuilder>,
    multi_bar: MultiProgress,
    jobs: Option<usize>,
}

impl MultiNodeClone {
    /// Construct new multi-node clone type from cluster definition.
    ///
    /// Extracts all node entries from cluster definition. Will set the number of threads/jobs that
    /// will be used during the cloning of all nodes, with [`None`] resulting the saturation of all
    /// CPU cores as much as possible.
    ///
    /// # Errors
    ///
    /// - Return [`Error::NoWayData`] if data directory path cannot be determined.
    pub fn new(cluster: &Cluster, jobs: Option<usize>) -> Result<Self> {
        let multi_bar = MultiProgress::new();
        let mut nodes: Vec<GitBuilder> = Vec::new();

        for (name, node) in cluster.nodes.iter() {
            let repo = GitBuilder::new(data_dir()?.join(name))
                .url(&node.url)
                .kind(node.deployment.clone())
                .authenticator(ProgressBarAuthenticator::new(ProgressBarKind::MultiBar(
                    multi_bar.clone(),
                )));

            nodes.push(repo);
        }

        Ok(Self { nodes, multi_bar, jobs })
    }

    /// Clone all node entries in cluster asynchronously.
    ///
    /// Shows clone progress for each clone tasks. Tasks may block if user needs to enter their
    /// credentials.
    ///
    /// # Invariants
    ///
    /// - Progress bars are properly finished no matter what.
    ///
    /// # Panics
    ///
    /// - Will panic if mutex guard fails to lock.
    /// - Will panic if mutex cannot be unwrapped to extract clone task result data.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Git2`] for clone task failure.
    ///     - Failed clone tasks will not cancel any active clone tasks that are not failing.
    ///     - Results are only collected until _all_ clone tasks have finished.
    pub async fn clone_all(self) -> Result<()> {
        let mut bars = Vec::new();
        let results = Arc::new(Mutex::new(Vec::new()));

        stream::iter(self.nodes)
            .for_each_concurrent(self.jobs, |node| {
                let results = results.clone();
                let bar = self.multi_bar.add(ProgressBar::no_length());
                bars.push(bar.clone());

                async move {
                    let result = tokio::spawn(async move { node.clone(&bar) }).await;
                    let mut guard = results.lock().unwrap();
                    guard.push(result);
                    drop(guard);
                }
            })
            .await;

        // INVARIANT: All progress bars should be finished properly.
        for bar in bars {
            bar.finish_and_clear();
        }

        // TODO: Report all failures instead of the first occurance of a failure.
        let results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
        let _ = results.into_iter().flatten().collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }
}

/// Tablize repository entry information in cluster.
pub struct TablizeCluster<'cluster> {
    root: &'cluster Root,
    cluster: &'cluster Cluster,
}

impl<'cluster> TablizeCluster<'cluster> {
    /// Construct new cluster tablizer.
    pub fn new(root: &'cluster Root, cluster: &'cluster Cluster) -> Self {
        Self { root, cluster }
    }

    /// List only names of all entries in cluster.
    ///
    /// # Errors
    ///
    /// - Will fail if a given root or node entry does not exist.
    pub fn names_only(&self) -> Result<()> {
        let mut builder = tabled::builder::Builder::new();
        builder.push_record(["<root>"]);

        // INVARIANT: All node entries must be sorted by name.
        let mut nodes: Vec<Node> = self
            .cluster
            .nodes
            .iter()
            .map(|(name, node)| Node::new_open(name, node))
            .collect::<Result<Vec<_>>>()?;
        nodes.sort_by(|a, b| a.name().cmp(b.name()));

        for node in &nodes {
            builder.push_record([node.name()]);
        }

        let mut table = builder.build();
        table.with(tabled::settings::Style::ascii_rounded());
        info!("Name only listing:\n{table}");

        Ok(())
    }

    /// List a wide range information about each entry in cluster.
    ///
    /// Will list the following information:
    ///
    /// - Deployment kind.
    /// - Entry name.
    /// - Deployment status.
    /// - Currently active branch.
    ///
    /// # Errors
    ///
    /// - Will fail if a given root or node entry does not exist.
    /// - Will fail if deployment status cannot be obtained.
    /// - Will fail if current branch cannot be obtained.
    #[instrument(skip(self))]
    pub fn fancy(&self) -> Result<()> {
        let mut builder = tabled::builder::Builder::new();
        let state = if is_deployed(&self.root.entry, &self.root.deployer.excluded, DeployState::WithExcluded)? {
            "deployed fully"
        } else {
            "deployed"
        };
        builder.push_record(["bare-alias", "<root>", state, self.root.current_branch()?.as_str()]);

        // INVARIANT: All node entries must be sorted by name.
        let mut nodes: Vec<Node> = self
            .cluster
            .nodes
            .iter()
            .map(|(name, node)| Node::new_open(name, node))
            .collect::<Result<Vec<_>>>()?;
        nodes.sort_by(|a, b| a.name().cmp(b.name()));

        for node in &nodes {
            let (deploy, state) = if node.entry.is_bare_alias() {
                if is_deployed(&node.entry, &node.deployer.excluded, DeployState::WithExcluded)? {
                    ("bare-alias", "deployed fully")
                } else if is_deployed(&node.entry, &node.deployer.excluded, DeployState::WithoutExcluded)? {
                    ("bare-alias", "deployed")
                } else {
                    ("bare-alias", "undeployed")
                }
            } else {
                ("[node:normal]", "undeployable")
            };
            builder.push_record([deploy, node.name(), state, node.current_branch()?.as_str()]);
        }

        let mut table = builder.build();
        table.with(tabled::settings::Style::ascii_rounded());
        info!("Fancy listing:\n{table}");

        Ok(())
    }
}


pub(crate) struct RepoEntry {
    name: String,
    repository: Repository,
    deployment: DeploymentKind,
    authenticator: GitAuthenticator,
}

impl RepoEntry {
    pub(crate) fn builder(name: impl Into<String>) -> Result<RepoEntryBuilder> {
        RepoEntryBuilder::new(name)
    }

    pub(crate) fn set_deployment(&mut self, kind: DeploymentKind) {
        self.deployment = kind;
    }

    pub(crate) fn is_empty(&self) -> Result<bool> {
        match self.repository.head() {
            Ok(_) => {
                let mut revwalk = self.repository.revwalk()?;
                revwalk.push_head()?;
                let mut no_commits = true;

                if revwalk.flatten().next().is_some() {
                    no_commits = false;
                }

                Ok(no_commits)
            }
            Err(_) => Ok(true),
        }
    }

    pub(crate) fn is_bare_alias(&self) -> bool {
        self.repository.is_bare() && self.deployment.is_bare()
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn path(&self) -> &Path {
        self.repository.path()
    }

    pub(crate) fn current_branch(&self) -> Result<String> {
        let shorthand = self.repository
            .head()?
            .shorthand_bytes()
            .to_vec();

        Ok(String::from_utf8_lossy(shorthand.as_slice()).into_owned())
    }

    #[instrument(skip(self, args))]
    pub(crate) fn gitcall_non_interactive(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<String> {
        let args = self.expand_bin_args(args);
        debug!("Run non interactive git with {args:?}");
        syscall_non_interactive("git", args)
    }

    #[instrument(skip(self, args))]
    pub(crate) fn gitcall_interactive(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<()> {
        info!("Interactive call to git for {:?}", self.name);
        let args = self.expand_bin_args(args);
        debug!("Run interactive git with {args:?}");
        syscall_interactive("git", args)
    }

    fn expand_bin_args(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Vec<OsString> {
        let gitdir = self.repository.path().to_string_lossy().into_owned().into();
        let path_args: Vec<OsString> = match &self.deployment {
            DeploymentKind::Normal => vec!["--git-dir".into(), gitdir],
            DeploymentKind::BareAlias(dir_alias) => {
                vec!["--git-dir".into(), gitdir, "--work-tree".into(), dir_alias.to_os_string()]
            }
        };

        let mut bin_args: Vec<OsString> = Vec::new();
        bin_args.extend(path_args);
        bin_args.extend(args.into_iter().map(Into::into));

        bin_args
    }
}

#[derive(Default, Debug)]
pub(crate) struct RepoEntryBuilder {
    name: String,
    path: PathBuf,
    deployment: DeploymentKind,
    authenticator: GitAuthenticator,
}

impl RepoEntryBuilder {
    pub(crate) fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let path = data_dir()?.join(&name);

        Ok(Self { name, path, ..Default::default() })
    }

    pub(crate) fn deployment(mut self, kind: DeploymentKind) -> Self {
        self.deployment = kind;
        self
    }

    /// Set authentication prompter.
    pub(crate) fn authentication_prompter(mut self, prompter: impl Prompter + Clone + 'static) -> Self {
        self.authenticator = self.authenticator.set_prompter(prompter);
        self
    }

    pub(crate) fn clone(self, url: impl AsRef<str>, bar: &ProgressBar) -> Result<RepoEntry> {
        let style = ProgressStyle::with_template(
            "{elapsed_precise:.green}  {msg:<50}  [{wide_bar:.yellow/blue}]",
        )?
        .progress_chars("-Cco.");
        bar.set_style(style);
        bar.set_message(format!("{} - {}", self.name, url.as_ref()));
        bar.enable_steady_tick(std::time::Duration::from_millis(100));

        let mut throttle = Instant::now();
        let config = Config::open_default()?;
        let mut rc = RemoteCallbacks::new();
        rc.credentials(self.authenticator.credentials(&config));
        rc.transfer_progress(|progress| {
            let stats = progress.to_owned();
            let bar_size = stats.total_objects() as u64;
            let bar_pos = stats.received_objects() as u64;
            if throttle.elapsed() > Duration::from_millis(50) {
                throttle = Instant::now();
                bar.set_length(bar_size);
                bar.set_position(bar_pos);
            }
            true
        });

        let mut fo = FetchOptions::new();
        fo.remote_callbacks(rc);

        let repository = RepoBuilder::new()
            .bare(self.deployment.is_bare())
            .fetch_options(fo)
            .clone(url.as_ref(), &self.path)?;

        if self.deployment.is_bare() {
            let mut config = repository.config()?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        Ok(RepoEntry {
            name: self.name,
            repository,
            deployment: self.deployment,
            authenticator: self.authenticator,
        })
    }

    pub(crate) fn init(self) -> Result<RepoEntry> {
        let mut opts = RepositoryInitOptions::new();
        opts.bare(self.deployment.is_bare());
        let repository = Repository::init_opts(&self.path, &opts)?;

        if self.deployment.is_bare() {
            let mut config = repository.config().map_err(Error::from)?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        Ok(RepoEntry {
            name: self.name,
            repository,
            deployment: self.deployment,
            authenticator: self.authenticator,
        })
    }

    pub(crate) fn open(self) -> Result<RepoEntry> {
        let repository = Repository::open(&self.path)?;

        Ok(RepoEntry {
            name: self.name,
            repository,
            deployment: self.deployment,
            authenticator: self.authenticator,
        })
    }
}

pub(crate) trait Deployment {
    fn deploy_action(
        &self,
        entry: &RepoEntry,
        excluded: &SparseCheckout,
        action: DeployAction
    ) -> Result<()>;
}

pub(crate) struct RepoEntryDeployer {
    excluded: SparseCheckout,
}

impl RepoEntryDeployer {
    pub(crate) fn new(entry: &RepoEntry) -> Self {
        let mut excluded = SparseCheckout::new();
        excluded.set_sparse_path(entry.path());

        Self { excluded }
    }

    pub(crate) fn add_excluded(&mut self, rules: impl IntoIterator<Item = impl Into<String>>) {
        self.excluded.add_exclusions(rules);
    }

    pub(crate) fn deploy_with(
        &self,
        deployer: impl Deployment,
        entry: &RepoEntry,
        action: DeployAction
    ) -> Result<()> {
        deployer.deploy_action(entry, &self.excluded, action)
    }
}


pub(crate) struct NormalDeployment;

impl Deployment for NormalDeployment {
    fn deploy_action(
        &self,
        entry: &RepoEntry,
        _excluded: &SparseCheckout,
        _action: DeployAction,
    ) -> Result<()> {
        if entry.is_bare_alias() {
            return Err(Error::NormalMixup { name: entry.name().into() });
        }

        info!("Repository {:?} is normal, no deployment needed", entry.name());

        Ok(())
    }
}

pub(crate) struct RootDeployment;

impl Deployment for RootDeployment {
    fn deploy_action(
        &self,
        entry: &RepoEntry,
        excluded: &SparseCheckout,
        action: DeployAction,
    ) -> Result<()> {
        if entry.is_empty()? {
            warn!("Root repository is empty, nothing to deploy");
            return Ok(());
        }

        if !entry.is_bare_alias() {
            return Err(Error::BareAliasMixup { name: entry.name().into() });
        }

        let msg = match action {
            DeployAction::Deploy => {
                if is_deployed(&entry, &excluded, DeployState::WithoutExcluded)? {
                    return Ok(())
                }

                warn!("Root repository not deployed");
                excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                format!("Deploy root, because it must always be deployed")
            }
            DeployAction::DeployAll => {
                if is_deployed(&entry, &excluded, DeployState::WithExcluded)? {
                    warn!("Root repository is already deployed fully");
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::IncludeAll)?;
                format!("Deploy all of root repository")
            }
            DeployAction::Undeploy => {
                warn!("Root repository cannot be undeployed");
                return Ok(());
            }
            DeployAction::UndeployExcludes => {
                if !is_deployed(&entry, &excluded, DeployState::WithExcluded)? {
                    warn!("Root repository excluded files are undeployed");
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                format!("Undeploy excluded files of root")
            }
        };

        let output = entry.gitcall_non_interactive(["checkout"])?;
        info!("{msg}\n{output}");

        Ok(())
    }
}

pub(crate) struct BareAliasDeployment;

impl Deployment for BareAliasDeployment {
    fn deploy_action(
        &self,
        entry: &RepoEntry,
        excluded: &SparseCheckout,
        action: DeployAction,
    ) -> Result<()> {
        if entry.is_empty()? {
            warn!("Repository {:?} is empty, nothing to deploy", entry.name());
            return Ok(());
        }

        if !entry.is_bare_alias() {
            return Err(Error::BareAliasMixup { name: entry.name().into() });
        }

        let msg = match action {
            DeployAction::Deploy => {
                if is_deployed(&entry, &excluded, DeployState::WithoutExcluded)? {
                    warn!("Repository {:?} is already deployed", entry.name);
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                format!("deploy {:?}", entry.name)
            }
            DeployAction::DeployAll => {
                if is_deployed(&entry, &excluded, DeployState::WithExcluded)? {
                    warn!("Repository {:?} is already deployed fully", entry.name);
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::IncludeAll)?;
                format!("deploy all of {:?}", entry.name)
            }
            DeployAction::Undeploy => {
                if !is_deployed(&entry, &excluded, DeployState::WithoutExcluded)? {
                    warn!("Repository {:?} is already undeployed fully", entry.name);
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::ExcludeAll)?;
                format!("undeploy {:?}", entry.name)
            }
            DeployAction::UndeployExcludes => {
                if !is_deployed(&entry, &excluded, DeployState::WithExcluded)? {
                    warn!("Repository {:?} excluded files are undeployed", entry.name);
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                format!("undeploy excluded files of {:?}", entry.name)
            }
        };

        let output = entry.gitcall_non_interactive(["checkout"])?;
        info!("{msg}\n{output}");

        Ok(())
    }
}

fn is_deployed(entry: &RepoEntry, excluded: &SparseCheckout, state: DeployState) -> Result<bool> {
    if entry.is_empty()? {
        return Ok(false);
    }

    let worktree = match &entry.deployment {
        DeploymentKind::Normal => return Ok(false),
        DeploymentKind::BareAlias(worktree) => worktree,
    };

    // Thank you Eric at https://www.hydrogen18.com/blog/list-all-files-git-repo-pygit2.html.
    let mut entries = Vec::new();
    let commit = entry.repository.head()?.peel_to_commit()?;
    let tree = commit.tree()?;
    for entry in tree.iter() {
        if let Some(filename) = entry.name() {
            entries.push(filename.to_string());
        }
    }

    if state == DeployState::WithoutExcluded {
        let excludes = glob_match(excluded.iter(), entries.iter());
        entries.retain(|x| !excludes.contains(x));
    }

    for entry in entries {
        let path = worktree.0.join(entry);
        if !path.exists() {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Git repository manager.
///
/// Wraps a Git repository to provide important functionality regarding the management, deployment,
/// and processing of repository data throughout the codebase.
pub(crate) struct Git {
    name: String,
    path: PathBuf,
    kind: DeploymentKind,
    url: String,
    excluded: SparseCheckout,
    authenticator: GitAuthenticator,
    repository: Repository,
}

impl Git {
    /// Construct new Git repository through builder.
    pub(crate) fn builder(path: impl Into<PathBuf>) -> GitBuilder {
        GitBuilder::new(path.into())
    }

    /// Set kind of repository for deployment.
    pub(crate) fn set_kind(&mut self, kind: DeploymentKind) {
        self.kind = kind;
    }

    /// Set files to exclude based on a set of sparsity rules.
    pub(crate) fn set_excluded(&mut self, rules: impl IntoIterator<Item = impl Into<String>>) {
        self.excluded.add_exclusions(rules);
    }

    /// Name of managed repository.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Path of managed repository.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Determine if repository is bare.
    pub(crate) fn is_bare(&self) -> bool {
        self.kind.is_bare() && self.repository.is_bare()
    }

    /// Determine if repository is empty.
    ///
    /// Empty in this case simply means that a repository has no commits.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Git2`] for any repository operation failure.
    pub(crate) fn is_empty(&self) -> Result<bool> {
        match self.repository.head() {
            Ok(_) => {
                let mut revwalk = self.repository.revwalk()?;
                revwalk.push_head()?;
                let mut no_commits = true;

                if revwalk.flatten().next().is_some() {
                    no_commits = false;
                }

                Ok(no_commits)
            }
            Err(_) => Ok(true),
        }
    }

    /// Determine if repository is currently deployed.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Git2`] for any repository operation failure.
    pub(crate) fn is_deployed(&self, state: DeployState) -> Result<bool> {
        if self.is_empty()? {
            return Ok(false);
        }

        let worktree = match &self.kind {
            DeploymentKind::Normal => return Ok(false),
            DeploymentKind::BareAlias(worktree) => worktree,
        };

        // Thank you Eric at https://www.hydrogen18.com/blog/list-all-files-git-repo-pygit2.html.
        let mut entries = Vec::new();
        let commit = self.repository.head()?.peel_to_commit()?;
        let tree = commit.tree()?;
        for entry in tree.iter() {
            if let Some(filename) = entry.name() {
                entries.push(filename.to_string());
            }
        }

        if state == DeployState::WithoutExcluded {
            let excludes = glob_match(self.excluded.iter(), entries.iter());
            entries.retain(|x| !excludes.contains(x));
        }

        for entry in entries {
            let path = worktree.0.join(entry);
            if !path.exists() {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Get current name of branch.
    ///
    /// # Errors
    ///
    /// - Will fail if HEAD is not pointing to a named branch.
    pub(crate) fn current_branch(&self) -> Result<String> {
        self.repository
            .head()
            .map_err(Error::from)?
            .shorthand()
            .map(Into::into)
            .ok_or(Error::Git2UnknownBranch { repo: self.name().into() })
    }

    /// Extract string data from target file in index.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Git2FileNotFound`] if file does not exist in index.
    /// - Return [`Error::Git2`] for any other failure with repository.
    ///
    /// [`Error::Git2FileNotFound`]: crate::Error::Git2FileNotFound
    /// [`Error::Git2`]: crate::Error::Git2
    #[instrument(skip(self, name))]
    pub(crate) fn extract_file_data(&self, name: impl AsRef<str>) -> Result<String> {
        if self.is_empty()? {
            warn!("Repository {:?} is empty, no {:?} file to extract", self.name, name.as_ref());
            return Ok(String::default());
        }

        let commit = self.repository.head()?.peel_to_commit()?;
        let tree = commit.tree()?;
        let entry = tree.get_name(name.as_ref()).ok_or(Error::NoClusterFile)?;
        let blob = entry.to_object(&self.repository)?.peel_to_blob()?;

        let content = String::from_utf8_lossy(blob.content()).into_owned();
        debug!("Extracted the following content from {:?}\n{content}", name.as_ref());

        Ok(content)
    }

    /// Deploy repository index to target worktree alias based on selected deployment action.
    ///
    /// Will not deploy a repository that is normal. Will also not perform target deployment action
    /// if that action was already performed on the repository's index beforehand.
    ///
    /// # Errors
    ///
    /// - Will fail if required sparsity rules cannot be written to sparse checkout file.
    /// - Will fail if repository index cannot be deployed for whatever reason.
    #[instrument(skip(self, action))]
    pub(crate) fn deploy(&self, action: DeployAction) -> Result<()> {
        if self.is_empty()? {
            warn!("Repository {:?} is empty, nothing to deploy", self.name());
            return Ok(());
        }

        if !self.is_bare() {
            warn!("Repository {:?} is normal, deployment unnecessary", self.path);
            return Ok(());
        }

        let msg = match action {
            DeployAction::Deploy => {
                if self.is_deployed(DeployState::WithoutExcluded)? {
                    warn!("Repository {:?} is already deployed", self.path);
                    return Ok(());
                }

                self.excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                format!("deploy {:?}", self.path)
            }
            DeployAction::DeployAll => {
                if self.is_deployed(DeployState::WithExcluded)? {
                    warn!("Repository {:?} is already deployed fully", self.path);
                    return Ok(());
                }

                self.excluded.write_rules(ExcludeAction::IncludeAll)?;
                format!("deploy all of {:?}", self.path)
            }
            DeployAction::Undeploy => {
                if !self.is_deployed(DeployState::WithoutExcluded)? {
                    warn!("Repository {:?} is already undeployed fully", self.path);
                    return Ok(());
                }

                self.excluded.write_rules(ExcludeAction::ExcludeAll)?;
                format!("undeploy {:?}", self.path)
            }
            DeployAction::UndeployExcludes => {
                if !self.is_deployed(DeployState::WithExcluded)? {
                    warn!("Repository {:?} excluded files are undeployed", self.path);
                    return Ok(());
                }

                self.excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                format!("undeploy excluded files of {:?}", self.path)
            }
        };

        let output = self.gitcall_non_interactive(["checkout"])?;
        info!("{msg}\n{output}");

        Ok(())
    }

    /// Call Git binary non-interactively.
    ///
    /// Will pipe stdout and stderr into a string that is returned to call for further evaluation.
    /// Caller cannot interact with Git binary at all. Mainly meant to feed Git binary one-liners
    /// whose output will be parsed and processed for further actions in codebase.
    ///
    /// # Errors
    ///
    /// - Will fail if Git binary cannot be found.
    /// - Will fail if provided arguments are invalid.
    #[instrument(skip(self, args))]
    pub fn gitcall_non_interactive(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<String> {
        let args = self.expand_bin_args(args);
        debug!("Run non interactive git with {args:?}");
        syscall_non_interactive("git", args)
    }

    /// Call git binary interactively.
    ///
    /// Allows caller to give control of current session to Git binary so user can properly
    /// interact with Git itself. Control is then given back to the OCD program once the user
    /// finishes interacting with Git.
    ///
    /// Once called, Git binary will inherit stdout and stderr from user's current environment. Any
    /// output that Git provides is produced interactively. Thus, there is no need to collect
    /// output, because user will have already seen it.
    ///
    /// # Errors
    ///
    /// - Will fail if Git binary cannot be found.
    /// - Will fail if provided arguments are invalid.
    #[instrument(skip(self, args))]
    pub fn gitcall_interactive(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<()> {
        info!("Interactive call to git for {:?}", self.name);
        let args = self.expand_bin_args(args);
        debug!("Run interactive git with {args:?}");
        syscall_interactive("git", args)
    }

    fn expand_bin_args(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Vec<OsString> {
        let gitdir = self.repository.path().to_string_lossy().into_owned().into();
        let path_args: Vec<OsString> = match &self.kind {
            DeploymentKind::Normal => vec!["--git-dir".into(), gitdir],
            DeploymentKind::BareAlias(dir_alias) => {
                vec!["--git-dir".into(), gitdir, "--work-tree".into(), dir_alias.to_os_string()]
            }
        };

        let mut bin_args: Vec<OsString> = Vec::new();
        bin_args.extend(path_args);
        bin_args.extend(args.into_iter().map(Into::into));

        bin_args
    }
}

impl std::fmt::Debug for Git {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Repository {{ name: {:?}, ", self.name)?;
        write!(f, "path: {:?}, ", self.path)?;
        write!(f, "kind: {:?} ", self.kind)?;
        write!(f, "url: {:?} ", self.url)?;
        write!(f, "excludes: {:?} ", self.excluded)?;
        write!(f, "authenticator: {:?} ", self.authenticator)?;
        writeln!(f, "repository: git2 stuff :D }}")
    }
}

/// Builder for [`Git`] type.
#[derive(Default, Debug)]
pub(crate) struct GitBuilder {
    name: String,
    path: PathBuf,
    kind: DeploymentKind,
    url: String,
    excluded: SparseCheckout,
    authenticator: GitAuthenticator,
}

impl GitBuilder {
    /// Construct new empty [`Git`] builder.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();

        Self { path, name, ..Default::default() }
    }

    /// Set kind of repository for deployment.
    pub(crate) fn kind(mut self, kind: DeploymentKind) -> Self {
        self.kind = kind;
        self
    }

    /// Set URL to clone from.
    pub(crate) fn url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// Set files to exclude based on a set of sparsity rules.
    pub(crate) fn excluded(mut self, rules: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.excluded.add_exclusions(rules);
        self
    }

    /// Set authentication prompter.
    pub(crate) fn authenticator(mut self, prompter: impl Prompter + Clone + 'static) -> Self {
        self.authenticator = self.authenticator.set_prompter(prompter);
        self
    }

    /// Build [`Git`] by cloning it.
    ///
    /// Will track progress of clone through a progress bar.
    ///
    /// # Errors
    ///
    /// - Return `Error::Git2` if repository could not be cloned.
    pub(crate) fn clone(mut self, bar: &ProgressBar) -> Result<Git> {
        let style = ProgressStyle::with_template(
            "{elapsed_precise:.green}  {msg:<50}  [{wide_bar:.yellow/blue}]",
        )?
        .progress_chars("-Cco.");
        bar.set_style(style);
        bar.set_message(self.url.clone());
        bar.enable_steady_tick(std::time::Duration::from_millis(100));

        let mut throttle = Instant::now();
        let config = Config::open_default()?;
        let mut rc = RemoteCallbacks::new();
        rc.credentials(self.authenticator.credentials(&config));
        rc.transfer_progress(|progress| {
            let stats = progress.to_owned();
            let bar_size = stats.total_objects() as u64;
            let bar_pos = stats.received_objects() as u64;
            if throttle.elapsed() > Duration::from_millis(50) {
                throttle = Instant::now();
                bar.set_length(bar_size);
                bar.set_position(bar_pos);
            }
            true
        });

        let mut fo = FetchOptions::new();
        fo.remote_callbacks(rc);

        let repository = RepoBuilder::new()
            .bare(self.kind.is_bare())
            .fetch_options(fo)
            .clone(&self.url, &self.path)?;

        if self.kind.is_bare() {
            let mut config = repository.config()?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        self.excluded.set_sparse_path(repository.path());

        Ok(Git {
            name: self.name,
            path: self.path,
            kind: self.kind,
            url: self.url,
            excluded: self.excluded,
            authenticator: self.authenticator,
            repository,
        })
    }

    /// Build [`Git`] by initializing new repository.
    ///
    /// # Errors
    ///
    /// - Return `Error::Git2` if repository could not be initialized.
    pub fn init(mut self) -> Result<Git> {
        let mut opts = RepositoryInitOptions::new();
        opts.bare(self.kind.is_bare());
        let repository = Repository::init_opts(&self.path, &opts)?;

        if self.kind.is_bare() {
            let mut config = repository.config().map_err(Error::from)?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        self.excluded.set_sparse_path(repository.path());

        Ok(Git {
            name: self.name,
            kind: self.kind,
            url: self.url,
            path: self.path,
            excluded: self.excluded,
            authenticator: self.authenticator,
            repository,
        })
    }

    /// Build [`Git`] by opening existing repository.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Git2`] if repository could not be opened.
    pub fn open(mut self) -> Result<Git> {
        let repository = Repository::open(&self.path)?;
        self.excluded.set_sparse_path(repository.path());

        Ok(Git {
            name: self.name,
            kind: self.kind,
            url: self.url,
            path: self.path,
            excluded: self.excluded,
            authenticator: self.authenticator,
            repository,
        })
    }
}

/// Variants of repository index deployment state.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum DeployState {
    /// Repository index is deployed without excluded files
    #[default]
    WithoutExcluded,

    /// Repository index is fully deployed with excluded files.
    WithExcluded,
}

/// Variants of repository index deployment.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum DeployAction {
    /// Deploy to target worktree excluding unwanted files.
    #[default]
    Deploy,

    /// Deploy entire index to target worktree.
    DeployAll,

    /// Undeploy entire index from target worktree.
    Undeploy,

    /// Only undeploy excluded files from target worktree.
    UndeployExcludes,
}

/// Manage authentication with progress bars.
///
/// Can handle single and multi progress bars based on [`ProgressBarKind`]. For any prompt to the
/// terminal, all progress bars will be blocked to prevent the creation of zombie lines.
#[derive(Clone)]
pub(crate) struct ProgressBarAuthenticator {
    bar_kind: ProgressBarKind,
}

impl ProgressBarAuthenticator {
    /// Construct new authentication prompt progress bar handler.
    pub(crate) fn new(bar_kind: ProgressBarKind) -> Self {
        Self { bar_kind }
    }
}

impl Prompter for ProgressBarAuthenticator {
    #[instrument(skip(self, url, _git_config))]
    fn prompt_username_password(
        &mut self,
        url: &str,
        _git_config: &git2::Config,
    ) -> Option<(String, String)> {
        let prompt = || -> Option<(String, String)> {
            info!("Authentication required for {url}");
            let username = Text::new("username").prompt().unwrap();
            let password = Password::new("password").without_confirmation().prompt().unwrap();
            Some((username, password))
        };

        match &self.bar_kind {
            ProgressBarKind::MultiBar(bar) => bar.suspend(prompt),
            ProgressBarKind::SingleBar(bar) => bar.suspend(prompt),
        }
    }

    #[instrument(skip(self, username, url, _git_config))]
    fn prompt_password(
        &mut self,
        username: &str,
        url: &str,
        _git_config: &git2::Config,
    ) -> Option<String> {
        let prompt = || -> Option<String> {
            info!("Authentication required for {url} for user {username}");
            let password = Password::new("password").without_confirmation().prompt().unwrap();
            Some(password)
        };

        match &self.bar_kind {
            ProgressBarKind::MultiBar(bar) => bar.suspend(prompt),
            ProgressBarKind::SingleBar(bar) => bar.suspend(prompt),
        }
    }

    #[instrument(skip(self, private_key_path, _git_config))]
    fn prompt_ssh_key_passphrase(
        &mut self,
        private_key_path: &Path,
        _git_config: &git2::Config,
    ) -> Option<String> {
        let prompt = || -> Option<String> {
            info!("Authentication required for {}", private_key_path.display());
            let password = Password::new("password").without_confirmation().prompt().unwrap();
            Some(password)
        };

        match &self.bar_kind {
            ProgressBarKind::MultiBar(bar) => bar.suspend(prompt),
            ProgressBarKind::SingleBar(bar) => bar.suspend(prompt),
        }
    }
}

/// Progress bar handler variants.
#[derive(Clone)]
pub(crate) enum ProgressBarKind {
    /// Need to handle only one progress bar.
    SingleBar(ProgressBar),

    /// Need to handle more than one progress bar.
    MultiBar(MultiProgress),
}

/// Sparse checkout handling.
///
/// Provide a simple way to manipulate the contents of Git's sparse checkout file. Internally, we
/// use sparse checkout to implement the file exclusion functionality when the user deploys a
/// repository to a target directory as a worktree alias.
///
/// Sparse checkout uses the gitignore syntax to define a set of _sparsity rules_. These sparsity
/// rules do not determine what gets excluded, but what gets included. A basic inverse of what the
/// gitignore rule patterns are supposed to do. However, to keep things consistent in the codebase,
/// we treat these sparsity rules as patterns of what to exclude from the index upon deployment.
///
/// Git does not provide a reliable way to remove sparsity rule entries from the sparse checkout
/// file of a repository. Thus, the file is directly manipulated such that the caller is expected
/// to call Git afterwards for the changes to take effect. Plus, we avoid having to make system
/// calls to Git to improve performance.
///
/// ## Drawbacks
///
/// In order to allow the user to exclude any file from any part of their index for a given
/// repository, OCD uses non-cone mode of Git's sparse checkout to achive this. The issue is that
/// non-cone mode is a deprecated feature whose performance serverly worsens the more refs it needs
/// to analyze for each sparsity rule (O(N * M) pattern matches). The maintainers of sparse
/// checkout will not be removing non-cone mode, but non-cone mode will not be receiving the same
/// updates and features as cone mode for future release of Git.
///
/// However, OCD makes the assumption that the user will utilize the cluster feature of the project
/// as much as possible. What makes a user's dotfiles big is the fact that they stuff all their
/// configurations in one monolithic repository, and deploy that huge repository where they need it
/// in one shot. With OCD's cluster feature, each dotfile configuration is split up across multiple
/// repositories. Thus, the performance penalty of non-cone mode is spread across multiple
/// repositories that will hopefully reduce its impact.
///
/// ## See also
///
/// - [git-sparse-checkout](https://git-scm.com/docs/git-sparse-checkout)
#[derive(Debug, Default, Clone)]
pub(crate) struct SparseCheckout {
    sparse_path: PathBuf,
    exclusion_rules: Vec<String>,
}

impl SparseCheckout {
    /// Construct new empty sparse checkout manipulator.
    pub(crate) fn new() -> Self {
        SparseCheckout::default()
    }

    /// Set expected path to sparse checkout file based on gitdir path.
    pub(crate) fn set_sparse_path(&mut self, gitdir: &Path) {
        self.sparse_path = gitdir.join("info/sparse-checkout");
    }

    /// Add list of sparsity rules to exclude files upon index checkout.
    pub(crate) fn add_exclusions(&mut self, rules: impl IntoIterator<Item = impl Into<String>>) {
        let mut vec = Vec::new();
        vec.extend(rules.into_iter().map(Into::into));
        self.exclusion_rules = vec;
    }

    /// Write sparsity rules based on exclusion action.
    ///
    /// Will create sparse checkout file at expected path if it does not exist.
    ///
    /// # Errors
    ///
    /// - Will fail if sparse checkout file cannot be created when needed.
    /// - Will fail if sparsity rules cannot be written to sparse checkout file.
    pub(crate) fn write_rules(&self, action: ExcludeAction) -> Result<()> {
        let rules: String = match action {
            ExcludeAction::ExcludeUnwanted => {
                let mut excluded = self.exclusion_rules.iter().fold(String::new(), |mut acc, u| {
                    writeln!(&mut acc, "!{u}").unwrap();
                    acc
                });
                excluded.insert_str(0, "/*\n");
                excluded
            }
            ExcludeAction::IncludeAll => "/*".into(),
            ExcludeAction::ExcludeAll => "".into(),
        };

        let mut file = File::create(&self.sparse_path)?;
        file.write_all(rules.as_bytes())?;

        Ok(())
    }

    /// Iterate through sparsity rules.
    ///
    /// Each pattern can be feed into [`glob_match`] if need be.
    ///
    /// [`glob_match`]: crate::utils::glob_match
    pub(crate) fn iter(&self) -> SparsityRuleIter<'_> {
        SparsityRuleIter { exclusion_rules: &self.exclusion_rules, index: 0 }
    }
}

/// Variants of exclusion actions for sparse checkout.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum ExcludeAction {
    /// Use given sparsity rules to exclude files that match.
    #[default]
    ExcludeUnwanted,

    /// Do not use sparsity rules so full index is checked out.
    IncludeAll,

    /// Use catchall sparsity rule to exclude entire index from checkout.
    ExcludeAll,
}

/// Iterator for sparsity rules.
///
/// Designed to make it easy to match glob data when walking through repository index.
#[derive(Debug)]
pub(crate) struct SparsityRuleIter<'rule> {
    exclusion_rules: &'rule Vec<String>,
    index: usize,
}

impl Iterator for SparsityRuleIter<'_> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.exclusion_rules.len() {
            return None;
        }

        let mut rule = self.exclusion_rules[self.index].clone();
        self.index += 1;

        // INVARIANT: Directory patterns match entire contents of directory itself through wildcard.
        if rule.ends_with('/') {
            rule.push('*');
        }

        Some(rule)
    }
}
