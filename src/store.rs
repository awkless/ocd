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
    glob_match,
    model::{config_dir, data_dir, Cluster, DeploymentKind, NodeEntry, RootEntry, WorkDirAlias},
};

use anyhow::{anyhow, Context, Result};
use auth_git2::{GitAuthenticator, Prompter};
use beau_collector::BeauCollector as _;
use futures::{stream, StreamExt};
use git2::{
    build::RepoBuilder, Config, FetchOptions, ObjectType, RemoteCallbacks, Repository,
    RepositoryInitOptions,
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::{Password, Text};
use std::{
    collections::VecDeque,
    ffi::{OsStr, OsString},
    fmt::Write as FmtWrite,
    fs::{remove_dir_all, File},
    io::Write as IoWrite,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tracing::{debug, info, instrument, trace, warn};

/// Root entry in repository store.
#[derive(Debug)]
pub struct Root {
    entry: RepoEntry,
    deployer: RepoEntryDeployer,
}

impl Root {
    /// Clone root repository from remote URL.
    ///
    /// Will deploy root repository by extracting internal root configuration file.
    ///
    /// # Errors
    ///
    /// - Will fail if clone itself fails.
    /// - Will fail if root configuration file could not be extracted.
    /// - Will fail if deployment of root fails.
    #[instrument(skip(url), level = "debug")]
    pub fn new_clone(url: impl AsRef<str>) -> Result<Self> {
        trace!("Clone new root repository");
        let bar = ProgressBar::no_length();
        let entry = RepoEntry::builder("root")?
            .url(url.as_ref())
            .deployment_kind(DeploymentKind::BareAlias)
            .work_dir_alias(WorkDirAlias::new(config_dir()?))
            .authentication_prompter(ProgressBarAuthenticator::new(ProgressBarKind::SingleBar(
                bar.clone(),
            )))
            .clone(&bar)?;
        bar.finish_and_clear();

        let deployer = RepoEntryDeployer::new(&entry);
        let mut root = Self { entry, deployer };
        let config = root.extract_root_config()?;

        std::fs::create_dir_all(config_dir()?)?;
        root.entry.set_deployment(DeploymentKind::BareAlias, config.settings.work_dir_alias);
        root.deployer.add_excluded(config.settings.excluded.iter().flatten());
        root.deployer.deploy_with(BareAliasDeployment, &root.entry, DeployAction::Deploy)?;

        Ok(root)
    }

    /// Open existing root in repository store.
    ///
    /// Will ensure that root is always deployed no matter what.
    ///
    /// # Errors
    ///
    /// - Will fail if root could not be opened.
    /// - Will fail if deployment check fails.
    pub fn new_open(entry: &RootEntry) -> Result<Self> {
        let repo = RepoEntry::builder("root")?.open()?;
        let deployer = RepoEntryDeployer::new(&repo);
        let mut root = Self { entry: repo, deployer };

        root.entry.set_deployment(DeploymentKind::BareAlias, entry.settings.work_dir_alias.clone());
        root.deployer.add_excluded(entry.settings.excluded.iter().flatten());
        root.deployer.deploy_with(RootDeployment, &root.entry, DeployAction::Deploy)?;

        Ok(root)
    }

    /// Initialize new empty root in repository store.
    ///
    /// # Errors
    ///
    /// Will fail if root could be initialized for whatever reason.
    #[instrument(skip(root), level = "debug")]
    pub fn new_init(root: &RootEntry) -> Result<Self> {
        info!("Initialize root repository");
        let entry = RepoEntry::builder("root")?
            .deployment_kind(DeploymentKind::BareAlias)
            .work_dir_alias(root.settings.work_dir_alias.clone())
            .init()?;
        let mut deployer = RepoEntryDeployer::new(&entry);
        deployer.add_excluded(root.settings.excluded.iter().flatten());

        Ok(Self { entry, deployer })
    }

    /// Deploy root according to given deployment action.
    ///
    /// Ensures that root cannot be undeployed.
    ///
    /// # Errors
    ///
    /// Will fail if deployment for given action fails for whatever reason.
    pub fn deploy(&self, action: DeployAction) -> Result<()> {
        self.deployer.deploy_with(RootDeployment, &self.entry, action)
    }

    /// Determine if root is currently deployed at specific state.
    ///
    /// # Errors
    ///
    /// Will fail if any given Git operation needed for this check to work fails for whatever
    /// reason.
    pub fn is_deployed(&self, state: DeployState) -> Result<bool> {
        is_deployed(&self.entry, &self.deployer.excluded, state)
    }

    /// Nuke root entry from repository store.
    ///
    /// # Errors
    ///
    /// - Will fail if root entry cannot be undeployed.
    /// - Will fail if root repository cannot be removed from repository store.
    #[instrument(skip(self), level = "debug")]
    pub fn nuke(&self) -> Result<()> {
        self.deployer.deploy_with(BareAliasDeployment, &self.entry, DeployAction::Undeploy)?;
        remove_dir_all(self.path())?;
        info!("Nuke {:?} from cluster", self.entry.name());

        Ok(())
    }

    /// Current branch of root repository.
    ///
    /// Uses lossy UTF-8 variation of branch pointed to by HEAD.
    ///
    /// # Errors
    ///
    /// Will fail if there is no index to use.
    pub fn current_branch(&self) -> Result<String> {
        self.entry.current_branch()
    }

    /// Get full path to root's gitdir.
    pub fn path(&self) -> &Path {
        self.entry.path()
    }

    /// Make interactive system call to user's Git binary.
    ///
    /// # Errors
    ///
    /// Will fail if system call fails, or Git was given invalid arguments.
    pub fn gitcall(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<()> {
        self.entry.gitcall_interactive(args)
    }

    /// Extract root configuration file.
    ///
    /// Extracts root configuration file based on most recent commit pointed to by HEAD. Will check
    /// for "root.toml" at top-level of repository, then ".config/ocd/root.toml" next. If root
    /// configuration file does not exist at either of these locations, then function errors out.
    ///
    /// # Errors
    ///
    ///  Will fail if root configuration file cannot be located at expected areas of repository.
    pub(crate) fn extract_root_config(&self) -> Result<RootEntry> {
        if self.entry.is_empty()? {
            warn!("Root is empty, defer to default settings");
            return RootEntry::try_default();
        }

        let commit = self.entry.repository.head()?.peel_to_commit()?;
        let tree = commit.tree()?;
        let blob = if let Some(entry) = tree.get_name("root.toml") {
            entry.to_object(&self.entry.repository)?.peel_to_blob()?
        } else {
            let entry = tree
                .get_path(PathBuf::from(".config/ocd/root.toml").as_path())
                .map_err(|_| anyhow!("Cannot locate 'root.toml' file"))?;
            entry.to_object(&self.entry.repository)?.peel_to_blob()?
        };

        let content = String::from_utf8_lossy(blob.content()).into_owned();
        let root: RootEntry = toml::de::from_str(&content)?;
        debug!("Extracted the following content from 'root.toml'\n{root:?}");

        Ok(root)
    }
}

/// Manage node repository in repository store.
#[derive(Debug)]
pub struct Node {
    entry: RepoEntry,
    deployer: RepoEntryDeployer,
}

impl Node {
    /// Initialize new node repository in repository store.
    ///
    /// # Errors
    ///
    /// Will fail if repository could not be initialized for whatever reason.
    #[instrument(skip(name, node), level = "debug")]
    pub fn new_init(name: impl AsRef<str>, node: &NodeEntry) -> Result<Self> {
        info!("Initialize node repository {:?}", name.as_ref());
        let entry = RepoEntry::builder(name.as_ref())?
            .deployment_kind(node.settings.deployment.kind.clone())
            .work_dir_alias(node.settings.deployment.work_dir_alias.clone())
            .init()?;
        let mut deployer = RepoEntryDeployer::new(&entry);
        deployer.add_excluded(node.settings.excluded.iter().flatten());

        Ok(Self { entry, deployer })
    }

    /// Construct new node by opening existing node repository.
    ///
    /// Will clone node repository if it does not already exist.
    ///
    /// # Errors
    ///
    /// - Will fail if clone itself fails when node is found to be missing.
    /// - Will fail if existing node cannot be opened for whatever reason.
    pub fn new_open(name: impl AsRef<str>, node: &NodeEntry) -> Result<Self> {
        let entry = if data_dir()?.join(name.as_ref()).exists() {
            RepoEntry::builder(name.as_ref())?
                .url(&node.settings.url)
                .deployment_kind(node.settings.deployment.kind.clone())
                .work_dir_alias(node.settings.deployment.work_dir_alias.clone())
                .open()?
        } else {
            let bar = ProgressBar::no_length();
            let entry = RepoEntry::builder(name.as_ref())?
                .url(&node.settings.url)
                .deployment_kind(node.settings.deployment.kind.clone())
                .work_dir_alias(node.settings.deployment.work_dir_alias.clone())
                .authentication_prompter(ProgressBarAuthenticator::new(ProgressBarKind::SingleBar(
                    bar.clone(),
                )))
                .clone(&bar)?;
            bar.finish_and_clear();
            entry
        };

        let mut deployer = RepoEntryDeployer::new(&entry);
        deployer.add_excluded(node.settings.excluded.iter().flatten());

        Ok(Self { entry, deployer })
    }

    /// Nuke node entry from repository store.
    ///
    /// # Errors
    ///
    /// - Will fail if node entry cannot be undeployed.
    /// - Will fail if node repository cannot be removed from repository store.
    #[instrument(skip(self), level = "debug")]
    pub fn nuke(&self) -> Result<()> {
        self.deploy(DeployAction::Undeploy)?;
        remove_dir_all(self.path())?;
        info!("Nuke node {:?} from cluster", self.entry.name());

        Ok(())
    }

    /// Path to node repository.
    pub fn path(&self) -> &Path {
        self.entry.path()
    }

    /// Name of node repository.
    pub fn name(&self) -> &str {
        self.entry.name()
    }

    /// Determine if node is bare-alias.
    pub fn is_bare_alias(&self) -> bool {
        self.entry.is_bare_alias()
    }

    /// Determine if node is currently deployed at specific state.
    ///
    /// # Errors
    ///
    /// Will fail if any given Git operation needed for this check to work fails for whatever
    /// reason.
    pub fn is_deployed(&self, state: DeployState) -> Result<bool> {
        is_deployed(&self.entry, &self.deployer.excluded, state)
    }

    /// Get current name of branch.
    ///
    /// # Errors
    ///
    /// Will fail if there is no index to use.
    pub fn current_branch(&self) -> Result<String> {
        self.entry.current_branch()
    }

    /// Deploy node repository.
    ///
    /// # Errors
    ///
    /// Will fail if deployment action fails for whatever reason.
    pub fn deploy(&self, action: DeployAction) -> Result<()> {
        match self.entry.deployment_kind {
            DeploymentKind::Normal => {
                self.deployer.deploy_with(NormalDeployment, &self.entry, action)
            }
            DeploymentKind::BareAlias => {
                self.deployer.deploy_with(BareAliasDeployment, &self.entry, action)
            }
        }
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
#[derive(Debug)]
pub struct MultiNodeClone {
    nodes: Vec<RepoEntryBuilder>,
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
    ///- Will fail if [`RepoEntryBuilder`] could not be constructed for a given node entry.
    pub fn new(cluster: &Cluster, jobs: Option<usize>) -> Result<Self> {
        let multi_bar = MultiProgress::new();
        let mut nodes: Vec<RepoEntryBuilder> = Vec::new();

        for (name, node) in &cluster.nodes {
            let repo = RepoEntryBuilder::new(name)?
                .url(&node.settings.url)
                .deployment_kind(node.settings.deployment.kind.clone())
                .work_dir_alias(node.settings.deployment.work_dir_alias.clone())
                .authentication_prompter(ProgressBarAuthenticator::new(ProgressBarKind::MultiBar(
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
    /// - Will fail for clone task failure.
    ///     - Failed clone tasks will not cancel any active clone tasks that are not failing.
    ///     - Results are only collected until _all_ clone tasks have finished.
    ///     - All errors are reported in one-shot.
    pub async fn clone_all(self) -> Result<()> {
        let mut bars = Vec::new();
        let results = Arc::new(Mutex::new(Vec::new()));

        stream::iter(self.nodes)
            .for_each_concurrent(self.jobs, |node| {
                let results = results.clone();
                let bar = self.multi_bar.add(ProgressBar::no_length());
                bars.push(bar.clone());

                async move {
                    let node_name = node.name.clone();
                    let result = tokio::spawn(async move { node.clone(&bar) }).await;
                    let mut guard = results.lock().unwrap();
                    guard.push(
                        result.map_err(|err| anyhow!("Failed to clone {node_name:?}: {err:?}")),
                    );
                    drop(guard);
                }
            })
            .await;

        // INVARIANT: All progress bars should be finished properly.
        for bar in bars {
            bar.finish_and_clear();
        }

        // INVARIANT: Collect and report _all_ failures encountered.
        let results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
        let _ = results.into_iter().flatten().bcollect::<Vec<_>>()?;

        Ok(())
    }
}

/// Tablize repository entry information in cluster.
#[derive(Debug)]
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
    #[instrument(skip(self), level = "debug")]
    pub fn fancy(&self) -> Result<()> {
        let mut builder = tabled::builder::Builder::new();
        let state = if is_deployed(
            &self.root.entry,
            &self.root.deployer.excluded,
            DeployState::WithExcluded,
        )? {
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
                } else if is_deployed(
                    &node.entry,
                    &node.deployer.excluded,
                    DeployState::WithoutExcluded,
                )? {
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

/// Entry representation of repository store.
///
/// Provides basic routines to create and manage repository entries in repository store of user's
/// cluster.
pub(crate) struct RepoEntry {
    name: String,
    repository: Repository,
    deployment_kind: DeploymentKind,
    work_dir_alias: WorkDirAlias,
    authenticator: GitAuthenticator,
}

impl RepoEntry {
    /// Use builder to construct new repository entry.
    pub(crate) fn builder(name: impl Into<String>) -> Result<RepoEntryBuilder> {
        RepoEntryBuilder::new(name)
    }

    /// Set deployment type for repository entry.
    pub(crate) fn set_deployment(
        &mut self,
        deployment_kind: DeploymentKind,
        work_dir_alias: WorkDirAlias,
    ) {
        self.deployment_kind = deployment_kind;
        self.work_dir_alias = work_dir_alias;
    }

    /// Check if repository entry is empty.
    ///
    /// A repository with no commits is considered to be empty.
    ///
    /// # Errors
    ///
    /// - Will fail if revwalk can not be performed.
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

    /// Check if repository is bare-alias.
    pub(crate) fn is_bare_alias(&self) -> bool {
        self.repository.is_bare() && self.deployment_kind.is_bare_alias()
    }

    /// Name of repository entry.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Absolute path to repository entry's gitdir.
    pub(crate) fn path(&self) -> &Path {
        self.repository.path()
    }

    /// Get name of current branch pointed to by HEAD.
    ///
    /// Returns current branch in lossy UTF-8 form.
    ///
    /// # Errors
    ///
    /// - Will fail if HEAD connot be determined.
    pub(crate) fn current_branch(&self) -> Result<String> {
        let shorthand = self.repository.head()?.shorthand_bytes().to_vec();
        Ok(String::from_utf8_lossy(shorthand.as_slice()).into_owned())
    }

    /// Perform non-interactive call to user's Git binary.
    ///
    /// Pipes stdout and stderr into a string for further manipulation.
    ///
    /// # Errors
    ///
    /// Will fail if call to Git binary fails, or Git binary was given invalid arguments.
    #[instrument(skip(self, args), level = "debug")]
    pub(crate) fn gitcall_non_interactive(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<String> {
        let args = self.expand_bin_args(args);
        debug!("Run non interactive git with {args:?}");
        syscall_non_interactive("git", args)
    }

    /// Perform interactive call to user's Git binary.
    ///
    /// Inherits user's shell environment, allowing for Git to prompt user for information
    /// interactively.
    ///
    /// # Errors
    ///
    /// Will fail if call to Git binary fails, or Git binary was given invalid arguments.
    #[instrument(skip(self, args), level = "debug")]
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
        let path_args: Vec<OsString> = match &self.deployment_kind {
            DeploymentKind::Normal => vec!["--git-dir".into(), gitdir],
            DeploymentKind::BareAlias => {
                vec![
                    "--git-dir".into(),
                    gitdir,
                    "--work-tree".into(),
                    self.work_dir_alias.to_os_string(),
                ]
            }
        };

        let mut bin_args: Vec<OsString> = Vec::new();
        bin_args.extend(path_args);
        bin_args.extend(args.into_iter().map(Into::into));

        bin_args
    }
}

impl std::fmt::Debug for RepoEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RepoEntry {{ name: {:?}, ", self.name)?;
        write!(f, "repository: (git2 stuff), ")?;
        write!(f, "deployment_kind: {:?} ", self.deployment_kind)?;
        write!(f, "work_dir_alias: {:?} ", self.work_dir_alias)?;
        writeln!(f, "authenticator: {:?} }}", self.authenticator)
    }
}

/// Builder for [`RepoEntry`].
#[derive(Debug)]
pub(crate) struct RepoEntryBuilder {
    name: String,
    path: PathBuf,
    url: String,
    deployment_kind: DeploymentKind,
    work_dir_alias: WorkDirAlias,
    authenticator: GitAuthenticator,
}

impl RepoEntryBuilder {
    /// Construct new builder.
    pub(crate) fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let path = data_dir()?.join(&name);
        Ok(Self {
            name,
            path,
            url: String::default(),
            deployment_kind: DeploymentKind::BareAlias,
            work_dir_alias: WorkDirAlias::try_default()?,
            authenticator: GitAuthenticator::default(),
        })
    }

    /// Set deployment settings for repository entry.
    pub(crate) fn deployment_kind(mut self, kind: DeploymentKind) -> Self {
        self.deployment_kind = kind;
        self
    }

    /// Set path to function as working directory alias.
    pub(crate) fn work_dir_alias(mut self, path: WorkDirAlias) -> Self {
        self.work_dir_alias = path;
        self
    }

    /// Set URL to clone from for repository entry.
    pub(crate) fn url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// Set authentication prompter.
    pub(crate) fn authentication_prompter(
        mut self,
        prompter: impl Prompter + Clone + 'static,
    ) -> Self {
        self.authenticator = self.authenticator.set_prompter(prompter);
        self
    }

    /// Clone repository entry from URL.
    ///
    /// Will show pretty progress bar of how long it is taking to perform the clone. Will also
    /// prompt the user for authentication if needed, which may pause any progress bars that are
    /// active.
    ///
    /// # Errors
    ///
    /// Will fail if given invalid URL, invalid credentials, or any other reason that may cause the
    /// clone to fail.
    pub(crate) fn clone(self, bar: &ProgressBar) -> Result<RepoEntry> {
        let style = ProgressStyle::with_template(
            "{elapsed_precise:.green}  {msg:<50}  [{wide_bar:.yellow/blue}]",
        )?
        .progress_chars("-Cco.");
        bar.set_style(style);
        bar.set_message(format!("{} - {}", self.name, self.url));
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
            .bare(self.deployment_kind.is_bare_alias())
            .fetch_options(fo)
            .clone(&self.url, &self.path)?;

        if self.deployment_kind.is_bare_alias() {
            let mut config = repository.config()?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        Ok(RepoEntry {
            name: self.name,
            repository,
            deployment_kind: self.deployment_kind,
            work_dir_alias: self.work_dir_alias,
            authenticator: self.authenticator,
        })
    }

    /// Initialize new repository entry.
    ///
    /// # Errors
    ///
    /// Will fail if repository cannot be  initialized properly.
    pub(crate) fn init(self) -> Result<RepoEntry> {
        let mut opts = RepositoryInitOptions::new();
        opts.bare(self.deployment_kind.is_bare_alias());
        let repository = Repository::init_opts(&self.path, &opts)?;

        if self.deployment_kind.is_bare_alias() {
            let mut config = repository.config()?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        Ok(RepoEntry {
            name: self.name,
            repository,
            deployment_kind: self.deployment_kind,
            work_dir_alias: self.work_dir_alias,
            authenticator: self.authenticator,
        })
    }

    /// Open existing repository entry.
    ///
    /// # Errors
    ///
    /// Will fail if repository cannot be opened for whatever reason.
    pub(crate) fn open(self) -> Result<RepoEntry> {
        let repository = Repository::open(&self.path)?;

        Ok(RepoEntry {
            name: self.name,
            repository,
            deployment_kind: self.deployment_kind,
            work_dir_alias: self.work_dir_alias,
            authenticator: self.authenticator,
        })
    }
}

/// Strategy for repository deployment.
pub(crate) trait Deployment {
    fn deploy_action(
        &self,
        entry: &RepoEntry,
        excluded: &SparseCheckout,
        action: DeployAction,
    ) -> Result<()>;
}

/// Handler for repository deployment strategies.
#[derive(Debug)]
pub(crate) struct RepoEntryDeployer {
    excluded: SparseCheckout,
}

impl RepoEntryDeployer {
    /// Construct new repository entry deployer.
    ///
    /// Sets sparse path based on given repository entry.
    pub(crate) fn new(entry: &RepoEntry) -> Self {
        let mut excluded = SparseCheckout::new();
        excluded.set_sparse_path(entry.path());

        Self { excluded }
    }

    /// Add exclusion rules for deployment.
    pub(crate) fn add_excluded(&mut self, rules: impl IntoIterator<Item = impl Into<String>>) {
        self.excluded.add_exclusions(rules);
    }

    /// Deploy with given strategy.
    ///
    /// # Errors
    ///
    /// Will fail if sparse-checkout fails with exclusion rules, or deployment strategy itself fails
    /// for whatever reason.
    pub(crate) fn deploy_with(
        &self,
        deployer: impl Deployment,
        entry: &RepoEntry,
        action: DeployAction,
    ) -> Result<()> {
        deployer.deploy_action(entry, &self.excluded, action)
    }
}

/// Deployment strategy for root repository.
///
/// ## Rules
///
/// 1. Root must always be deployed.
/// 2. Root cannot be undeployed.
/// 3. Root is always bare-alias.
/// 4. Excluded files can be either deployed or undeployed.
///     1. Excluded files are not deployed by default.
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
            return Err(anyhow!(
                "Root repository was somehow defined as normal when it should be bare-alias"
            ));
        }

        let msg = match action {
            DeployAction::Deploy => {
                if is_deployed(entry, excluded, DeployState::WithoutExcluded)? {
                    return Ok(());
                }

                warn!("Root repository not deployed");
                excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                "Deploy root, because it must always be deployed".to_string()
            }
            DeployAction::DeployAll => {
                if is_deployed(entry, excluded, DeployState::WithExcluded)? {
                    warn!("Root repository is already deployed fully");
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::IncludeAll)?;
                "Deploy all of root repository".to_string()
            }
            DeployAction::Undeploy => {
                warn!("Root repository cannot be undeployed");
                return Ok(());
            }
            DeployAction::UndeployExcludes => {
                if !is_deployed(entry, excluded, DeployState::WithExcluded)? {
                    warn!("Root repository excluded files are undeployed");
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                "Undeploy excluded files of root".to_string()
            }
        };

        let output = entry.gitcall_non_interactive(["checkout"])?;
        info!("{msg}\n{output}");

        Ok(())
    }
}

/// Deployment strategy for normal repositories.
///
/// ## Rules
///
/// 1. Normal repositories cannot be deployed.
/// 3. Make sure normal repository is actually defined to be normal.
pub(crate) struct NormalDeployment;

impl Deployment for NormalDeployment {
    fn deploy_action(
        &self,
        entry: &RepoEntry,
        _excluded: &SparseCheckout,
        _action: DeployAction,
    ) -> Result<()> {
        if entry.is_bare_alias() {
            return Err(anyhow!(
                "Repository {:?} defined as normal, but is bare-alias",
                entry.name
            ));
        }

        info!("Repository {:?} is normal, no deployment needed", entry.name());

        Ok(())
    }
}

/// Deployment strategy for bare-alias repositories.
///
/// ## Rules
///
/// 1. Bare-alias repositories can either be deployed or undeployed.
///     1. Excluded files are not included unless specified with deployment by default.
/// 2. Make sure bare-alias repository is actually defined to be bare-alias.
/// 3. Skip deployment if bare-alias repository is already deployed.
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
            return Err(anyhow!(
                "Repository {:?} defined as bare-alias, but is normal",
                entry.name
            ));
        }

        let msg = match action {
            DeployAction::Deploy => {
                if is_deployed(entry, excluded, DeployState::WithoutExcluded)? {
                    warn!("Repository {:?} is already deployed", entry.name);
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                format!("Deploy {:?}", entry.name)
            }
            DeployAction::DeployAll => {
                if is_deployed(entry, excluded, DeployState::WithExcluded)? {
                    warn!("Repository {:?} is already deployed fully", entry.name);
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::IncludeAll)?;
                format!("Deploy all of {:?}", entry.name)
            }
            DeployAction::Undeploy => {
                if !is_deployed(entry, excluded, DeployState::WithoutExcluded)? {
                    warn!("Repository {:?} is already undeployed fully", entry.name);
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::ExcludeAll)?;
                format!("Undeploy {:?}", entry.name)
            }
            DeployAction::UndeployExcludes => {
                if is_deployed(entry, excluded, DeployState::WithExcluded)? {
                    warn!("Repository {:?} excluded files are already undeployed", entry.name);
                    return Ok(());
                }

                excluded.write_rules(ExcludeAction::ExcludeUnwanted)?;
                format!("Undeploy excluded files of {:?}", entry.name)
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

    let work_dir_alias = match &entry.deployment_kind {
        DeploymentKind::Normal => return Ok(false),
        DeploymentKind::BareAlias => &entry.work_dir_alias,
    };

    let mut entries: Vec<String> =
        list_file_paths(entry)?.into_iter().map(|p| p.to_string_lossy().into_owned()).collect();

    if state == DeployState::WithoutExcluded {
        let result = glob_match(excluded.iter(), entries.iter());
        entries.retain(|x| !result.contains(x));
    }

    for entry in entries {
        let path = work_dir_alias.0.join(entry);
        if !path.exists() {
            return Ok(false);
        }
    }

    Ok(true)
}

// Thank you Eric at https://www.hydrogen18.com/blog/list-all-files-git-repo-pygit2.html.
fn list_file_paths(entry: &RepoEntry) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    let commit = entry.repository.head()?.peel_to_commit()?;
    let tree = commit.tree()?;
    let mut trees_and_paths = VecDeque::new();
    trees_and_paths.push_front((tree, PathBuf::new()));

    // Iterate through all trees of repository entry, inserting full paths to each file blob in a
    // given tree until queue is exhausted.
    while let Some((tree, path)) = trees_and_paths.pop_front() {
        for tree_entry in &tree {
            match tree_entry.kind() {
                // Insert tree object into next iteration of queue...
                Some(ObjectType::Tree) => {
                    let next_tree = entry.repository.find_tree(tree_entry.id())?;
                    let next_path = path.join(bytes_to_path(tree_entry.name_bytes()));
                    trees_and_paths.push_front((next_tree, next_path));
                }
                // Insert filename with full path into path entry list...
                Some(ObjectType::Blob) => {
                    let full_path = path.join(bytes_to_path(tree_entry.name_bytes()));
                    entries.push(full_path);
                }
                _ => continue,
            }
        }
    }

    Ok(entries)
}

// Thanks from:
//
// https://github.com/rust-lang/git2-rs/blob/5bc3baa9694a94db2ca9cc256b5bce8a215f9013/
// src/util.rs#L85
#[cfg(unix)]
fn bytes_to_path(bytes: &[u8]) -> &Path {
    use std::os::unix::prelude::*;
    Path::new(OsStr::from_bytes(bytes))
}
#[cfg(windows)]
fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    use std::str;
    Path::new(str::from_utf8(bytes).unwrap())
}

/// Variants of repository index deployment state.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DeployState {
    /// Repository index is deployed without excluded files
    #[default]
    WithoutExcluded,

    /// Repository index is fully deployed with excluded files.
    WithExcluded,
}

/// Variants of repository index deployment.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DeployAction {
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
    #[instrument(skip(self, url, _git_config), level = "debug")]
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

    #[instrument(skip(self, username, url, _git_config), level = "debug")]
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

    #[instrument(skip(self, private_key_path, _git_config), level = "debug")]
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
            ExcludeAction::ExcludeAll => String::default(),
        };

        let mut file = File::create(&self.sparse_path)
            .with_context(|| "Failed to create sparse checkout file")?;
        file.write_all(rules.as_bytes()).with_context(|| "Failed to write sparsity rules")?;

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

fn syscall_non_interactive(
    cmd: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<String> {
    let output = Command::new(cmd.as_ref()).args(args).output()?;
    let stdout = String::from_utf8_lossy(output.stdout.as_slice()).into_owned();
    let stderr = String::from_utf8_lossy(output.stderr.as_slice()).into_owned();
    let mut message = String::new();

    if !stdout.is_empty() {
        message.push_str(format!("stdout: {stdout}").as_str());
    }

    if !stderr.is_empty() {
        message.push_str(format!("stderr: {stderr}").as_str());
    }

    if !output.status.success() {
        return Err(anyhow!("Command {:?} failed:\n{message}", cmd.as_ref()));
    }

    // INVARIANT: Chomp trailing newlines.
    let message = message
        .strip_suffix("\r\n")
        .or(message.strip_suffix('\n'))
        .map(ToString::to_string)
        .unwrap_or(message);

    Ok(message)
}

fn syscall_interactive(
    cmd: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<()> {
    let status = Command::new(cmd.as_ref()).args(args).spawn()?.wait()?;

    if !status.success() {
        return Err(anyhow!("Command {:?} failed", cmd.as_ref()));
    }

    Ok(())
}
