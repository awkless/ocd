// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

//! Version control system management.
//!
//! This module provides utilities to manipulate version control system (VCS) repository data for a
//! given cluster.  Currently, OCD mainly targets Git as the primary VCS of choice. Thus, the code
//! here makes it easy to manipulate Git repository data in a fashion that makes sense to OCD's
//! cluster configuration model.
//!
//! # Cluster structure
//!
//! The OCD tool operates on a __cluster__. A _cluster_ is a collection of Git repositories that
//! can be deployed together. The cluster is comprised of three repository types: __normal__,
//! __bare-alias__, and __root__. A _normal_ repository is just a regular Git repository whose
//! gitdir and worktree point to the same path. A _bare-alias_ repository is a bare Git repository
//! that uses a target directory as an alias of a worktree. That target directory can be treated
//! like a Git repository without initilization through the OCD tool itself.
//!
//! Finally, a _root_ repository is very special. It represents the root of the cluster itself. It
//! is responsible for containing the cluster configuration file that this module is meant to
//! handle. Thus, all repository deployment for a given cluster definition originates right here in
//! the root repository. However, a cluster can only have _one_ root, i.e., one repository
//! containing one copy of the cluster configuration file to deploy from.
//!
//! The cluster itself is defined through a special configuration file that the user writes and
//! maintains themselves. It contains all important configuration settings for each repository in
//! the cluster such that these settings determine how OCD will manage and deploy each repository,
//! including the root repository itself.
//!
//! The concept of a cluster provides the user with a lot of flexibility in how they choose to
//! organize their dotfile configurations. The user can store dotfiles in separate repositories and
//! plug them into a given cluster whenever they want. The user can also maintain a monolithic
//! repository containing every possible configuration file they use. Whatever method of
//! organization the user chooses, the OCD tool's cluster configuration model will provide flexible
//! and adaptable support.
//!
//! See [`cluster`](crate::cluster) for more information about OCD's cluster configuration model.

use crate::{
    cluster::{Cluster, Node},
    utils::{syscall_interactive, syscall_non_interactive, DirLayout},
};

use anyhow::{anyhow, Context, Result};
use auth_git2::{GitAuthenticator, Prompter};
use beau_collector::BeauCollector as _;
use futures::{stream, StreamExt};
use git2::{build::RepoBuilder, FetchOptions, RemoteCallbacks, Repository, RepositoryInitOptions};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::{Password, Text};
use std::{
    ffi::OsString,
    fmt::Write as FmtWrite,
    fs::File,
    io::Write as IoWrite,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Manage root repository.
#[derive(Debug, Default, Clone)]
pub struct RootRepo(Git);

impl RootRepo {
    /// Construct new root repository by cloning it.
    ///
    /// Will automatically set configuration settings based on cluster configuration file.
    ///
    /// # Errors
    ///
    /// Will fail if given invalid URL, or repository does not contain a cluster configuration file
    /// to deploy.
    pub fn new_clone(url: impl AsRef<str>, dirs: &DirLayout) -> Result<Self> {
        let bar = ProgressBar::no_length();
        let git = Git::new("root", dirs)
            .with_url(url.as_ref())
            .with_kind(RepoKind::Bare)
            .with_auth_prompt(ProgressBarAuth::new(ProgressBarKind::SingleBar(bar.clone())));
        git.clone_with_progress(&bar)?;
        bar.finish_and_clear();

        let mut root = Self(git);
        let cluster = root.get_cluster()?;
        let worktree = cluster.root.worktree.unwrap_or(dirs.config().to_path_buf());
        root.0 = root.0.with_kind(RepoKind::BareAlias(AliasDir::new(worktree)));
        root.0 = root.0.with_excludes(cluster.root.excludes.iter().flatten());

        Ok(root)
    }

    /// Construct new root repository by opening it.
    ///
    /// Will automatically set configuration settings based on cluster configuration file.
    ///
    /// # Errors
    ///
    /// Will fail if root repository does not exist for some reason.
    pub fn new_open(dirs: &DirLayout) -> Result<Self> {
        if !dirs.data().join("root").exists() {
            return Err(anyhow!("Root does not exist"));
        }

        let git = Git::new("root", dirs).with_kind(RepoKind::Bare);
        let mut root = Self(git);
        let cluster = root.get_cluster()?;
        let worktree = cluster.root.worktree.unwrap_or(dirs.config().to_path_buf());
        root.0 = root.0.with_kind(RepoKind::BareAlias(AliasDir::new(worktree)));
        root.0 = root.0.with_excludes(cluster.root.excludes.iter().flatten());

        Ok(root)
    }

    /// Construct new root repository from existing cluster.
    pub fn from_cluster(cluster: &Cluster, dirs: &DirLayout) -> Self {
        let worktree = cluster.root.worktree.as_ref().map_or(dirs.config(), |p| p.as_ref());
        let git = Git::new("root", dirs)
            .with_kind(RepoKind::BareAlias(AliasDir::new(worktree)))
            .with_excludes(cluster.root.excludes.iter().flatten());

        Self(git)
    }

    /// Extract cluster configuration file.
    ///
    /// # Errors
    ///
    /// Will fail if repository does not contain a cluster configuration file.
    pub fn get_cluster(&self) -> Result<Cluster> {
        self.0
            .bincall_non_interactive(["cat-file", "-p", "@:cluster.toml"])?
            .replace("stdout:", "")
            .parse::<Cluster>()
    }

    /// Initialize new root repository.
    ///
    /// # Errors
    ///
    /// Will fail if repository cannot be initialized for whatever reason.
    pub fn init(&self) -> Result<()> {
        log::info!("initialize root repository {}", self.0.path.display());
        self.0.init()
    }

    /// Determine how to deploy index of repository.
    ///
    /// # Errors
    ///
    /// Will fail if sparse checkout fails.
    pub fn index_deployment(&self, action: Deployment) -> Result<()> {
        self.0.index_deployment(action)
    }

    /// Call Git binary.
    ///
    /// Logs any data written to stdout or stderr.
    ///
    /// # Errors
    ///
    /// Will fail if Git binary cannot be found, or provided arguments are invalid to Git binary
    /// itself.
    pub fn gitcall(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<()> {
        self.0.bincall_interactive(args)
    }
}

/// Manage node repositories.
#[derive(Debug, Default, Clone)]
pub struct NodeRepo(Git);

impl NodeRepo {
    /// Construct new node repository from [`Node`].
    ///
    /// Extracts deserialized node data needed to manage a node repository.
    ///
    /// [`Node`]: crate::cluster::Node
    pub fn new(name: &str, node: &Node, dirs: &DirLayout) -> Self {
        let kind = if node.bare_alias {
            let path = node.worktree.as_ref().map_or(dirs.home(), |p| p.as_ref());
            RepoKind::BareAlias(AliasDir::new(path))
        } else {
            RepoKind::Normal
        };

        let git = Git::new(name, dirs).with_kind(kind).with_url(node.url.clone());

        Self(git)
    }

    /// Attch progress bar.
    ///
    /// Mainly used for keep track of clone progress with credential prompting.
    pub(crate) fn with_progress_bar(mut self, kind: ProgressBarKind) -> Self {
        self.0 = self.0.with_auth_prompt(ProgressBarAuth::new(kind));
        self
    }

    /// Initialize new node repository.
    ///
    /// # Errors
    ///
    /// Will fail if repository cannot be initialized for whatever reason.
    pub fn init(&self) -> Result<()> {
        log::info!("initialize node repository {}", self.0.path.display());
        self.0.init()
    }

    /// Determine how to deploy index of repository.
    ///
    /// # Errors
    ///
    /// Will fail if sparse checkout fails.
    pub fn index_deployment(&self, action: Deployment) -> Result<()> {
        self.0.index_deployment(action)
    }

    /// Call Git binary.
    ///
    /// Logs any data written to stdout or stderr.
    ///
    /// # Errors
    ///
    /// Will fail if Git binary cannot be found, or provided arguments are invalid to Git binary
    /// itself.
    pub fn gitcall(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<()> {
        self.0.bincall_interactive(args)
    }
}

/// Clone all nodes in cluster asynchronously.
pub struct MultiNodeClone {
    nodes: Vec<NodeRepo>,
    multi_bar: MultiProgress,
}

impl MultiNodeClone {
    /// Construct new multi-node clone type.
    ///
    /// Extracts all nodes from cluster to clone them with progress bar support.
    pub fn new(cluster: &Cluster, dirs: &DirLayout) -> Self {
        let multi_bar = MultiProgress::new();
        let nodes: Vec<NodeRepo> = cluster
            .nodes
            .iter()
            .map(|(name, node)| {
                NodeRepo::new(name, node, dirs)
                    .with_progress_bar(ProgressBarKind::MultiBar(multi_bar.clone()))
            })
            .collect();
        Self { nodes, multi_bar }
    }

    /// Clone all node repositories asynchronously.
    ///
    /// Shows clone progress for each clone task. Clears each progress bar after a task is
    /// finished. Tasks may block if user needs to enter their credentials.
    ///
    /// # Errors
    ///
    /// Will fail if any clone task fails. However, it will not cancel any active clone tasks that
    /// are not failing. Instead it will collect all failed tasks and report them in one shot after
    /// attempting to clone all node repositories.
    pub async fn clone_all(self, jobs: Option<usize>) -> Result<()> {
        let mut bars = Vec::new();
        let results = Arc::new(Mutex::new(Vec::new()));

        stream::iter(self.nodes)
            .for_each_concurrent(jobs, |node| {
                let bar = self.multi_bar.add(ProgressBar::no_length());

                bars.push(bar.clone());
                let results = results.clone();

                async move {
                    let result = tokio::spawn(async move {
                        node.0
                            .clone_with_progress(&bar)
                            .with_context(|| format!("Failed to clone {}", node.0.url))
                    })
                    .await;

                    let mut guard = results.lock().unwrap();
                    guard.push(result);
                    drop(guard);
                }
            })
            .await;

        for bar in bars {
            bar.finish_and_clear();
        }

        let results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
        let _ = results.into_iter().flatten().bcollect::<Vec<_>>()?;

        Ok(())
    }
}

/// Git repository manager.
#[derive(Debug, Default, Clone)]
pub struct Git {
    path: PathBuf,
    kind: RepoKind,
    url: String,
    auth: GitAuthenticator,
    sparsity: SparseManip,
}

impl Git {
    /// Construct new Git repository manager to manage target repository by name.
    pub fn new(repo: &str, dirs: &DirLayout) -> Self {
        Self { path: dirs.data().join(repo), ..Default::default() }
    }

    /// Set repository kind for repository.
    ///
    /// Determines how a repository will be managed and deployed. This method also sets the
    /// sparsity file path as well.
    pub fn with_kind(mut self, kind: RepoKind) -> Self {
        self.kind = kind;
        self.sparsity.set_sparse_path(self.path.as_ref(), &self.kind);
        self
    }

    /// Set URL to clone repository from.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// Set authentication prompter.
    ///
    /// Typically, the prompter being used should block progress bar output to prevent zombie lines.
    pub(crate) fn with_auth_prompt(mut self, prompter: impl Prompter + Clone + 'static) -> Self {
        self.auth = self.auth.set_prompter(prompter);
        self
    }

    /// Set exclude files.
    ///
    /// Add a list of files to exclude from sparse checkout upon deployment of repository.
    pub(crate) fn with_excludes(
        mut self,
        unwanted: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.sparsity.add_unwanted(unwanted);
        self
    }

    /// Get repository kind.
    pub fn kind(&self) -> &RepoKind {
        &self.kind
    }

    /// Get URL to clone from.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Get path to repository.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Initialize new empty repository.
    ///
    /// Will enable sparse checkout, and disable untracked file status for bare or bare-alias
    /// repositories.
    ///
    /// # Errors
    ///
    /// - Will fail if repository cannot be initialized at target path.
    /// - Will fail if repository cannot set default configuration settings when needed.
    pub fn init(&self) -> Result<()> {
        let mut opts = RepositoryInitOptions::new();
        opts.bare(self.kind.is_bare());
        let repo = Repository::init_opts(&self.path, &opts)?;

        if self.kind.is_bare() {
            let mut config = repo.config()?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        Ok(())
    }

    /// Clone repository with progress bar output.
    ///
    /// Performs Git clone without system call, with a progress bar to interactively show how long
    /// the clone is taking for the user. Method may prompt user for credentials if it cannot be
    /// automatically determined. This prompt may occur through external program or through the
    /// current terminal the user is running OCD on.
    ///
    /// # Errors
    ///
    /// Will fail if repository cannot be cloned for whatever reason. May also fail if user does
    /// not provide valid credentials when prompted.
    pub fn clone_with_progress(&self, bar: &ProgressBar) -> Result<()> {
        let style = ProgressStyle::with_template(
            "{elapsed_precise:.green}  {msg:<50}  [{wide_bar:.yellow/blue}]",
        )
        .unwrap()
        .progress_chars("-Cco.");
        bar.set_style(style);
        bar.set_message(self.url.clone());
        bar.enable_steady_tick(Duration::from_millis(100));

        let mut throttle = Instant::now();
        let config = git2::Config::open_default()?;
        let mut rc = RemoteCallbacks::new();
        rc.credentials(self.auth.credentials(&config));
        rc.transfer_progress(|progress| {
            let stats = progress.to_owned();
            let bar_size = stats.total_objects() as u64;
            let bar_pos = stats.received_objects() as u64;
            if throttle.elapsed() > Duration::from_millis(100) {
                throttle = Instant::now();
                bar.set_length(bar_size);
                bar.set_position(bar_pos);
            }
            true
        });

        let mut fo = FetchOptions::new();
        fo.remote_callbacks(rc);

        let repo = RepoBuilder::new()
            .bare(self.kind.is_bare())
            .fetch_options(fo)
            .clone(&self.url, self.path.as_ref())?;

        if matches!(self.kind, RepoKind::BareAlias(..) | RepoKind::Bare) {
            let mut config = repo.config()?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        Ok(())
    }

    /// Determine how to deploy index of repository.
    ///
    /// # Errors
    ///
    /// Will fail if sparse checkout fails.
    pub fn index_deployment(&self, action: Deployment) -> Result<()> {
        let msg = match action {
            Deployment::Deploy => {
                self.sparsity.exclude_unwanted()?;
                format!("deploy {}", self.path.display())
            }
            Deployment::Undeploy => {
                self.sparsity.exclude_all()?;
                format!("undeploy {}", self.path.display())
            }
            Deployment::DeployAll => {
                self.sparsity.include_all()?;
                format!("deploy all of {}", self.path.display())
            }
            Deployment::UndeployExcludes => {
                self.sparsity.exclude_unwanted()?;
                format!("undeploy excluded files of {}", self.path.display())
            }
        };

        let output = self.bincall_non_interactive(["checkout"])?;
        log::info!("{msg}\n{output}");

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
    pub fn bincall_non_interactive(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<String> {
        let args = self.expand_bin_args(args);
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
    /// output, because will have already seen it.
    ///
    /// # Errors
    ///
    /// - Will fail if Git binary cannot be found.
    /// - Will fail if provided arguments are invalid.
    pub fn bincall_interactive(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<()> {
        log::info!("interactive call to git for {}", self.path.display());
        let args = self.expand_bin_args(args);
        syscall_interactive("git", args)
    }

    fn expand_bin_args(
        &self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Vec<OsString> {
        let gitdir: OsString =
            if self.kind == RepoKind::Normal { self.path.join(".git") } else { self.path.clone() }
                .to_string_lossy()
                .into_owned()
                .into();

        let path_args: Vec<OsString> = match &self.kind {
            RepoKind::Normal | RepoKind::Bare => vec!["--git-dir".into(), gitdir],
            RepoKind::BareAlias(alias) => {
                vec!["--git-dir".into(), gitdir, "--work-tree".into(), alias.to_os_string()]
            }
        };

        let mut bin_args: Vec<OsString> = Vec::new();
        bin_args.extend(path_args);
        bin_args.extend(args.into_iter().map(Into::into));

        bin_args
    }
}

/// Determine how to treat repository.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub enum RepoKind {
    /// Normal Git repository whose gitdir and worktree point to same path.
    #[default]
    Normal,

    /// Normal bare Git repository with no worktree.
    Bare,

    /// Bare Git repository that uses a target directory as an alias for a worktree.
    BareAlias(AliasDir),
}

/// Methods of repository index deployment.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Deployment {
    /// Deply to target worktree excluding unwanted files.
    #[default]
    Deploy,

    /// Deploy entire index to target worktree.
    DeployAll,

    /// Undeploy entire index from target worktree.
    Undeploy,

    /// Only undeploy excluded files from target worktree.
    UndeployExcludes,
}

impl RepoKind {
    fn is_bare(&self) -> bool {
        match self {
            RepoKind::Normal => false,
            RepoKind::Bare | RepoKind::BareAlias(_) => true,
        }
    }
}

/// Alias directory path representation.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct AliasDir(pub PathBuf);

impl AliasDir {
    /// Contruct new alias directory path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Convert path to [`OsString`] lossy.
    pub fn to_os_string(&self) -> OsString {
        OsString::from(self.0.to_string_lossy().into_owned())
    }
}

/// Manage authentication prompt for progress bars.
///
/// Can handle single and multi progress bars based on [`ProgressBarKind`]. For any prompt to the
/// terminal, all progress bars will be blocked to prevent the creation of zombie lines.
#[derive(Clone)]
pub(crate) struct ProgressBarAuth {
    bar_kind: ProgressBarKind,
}

impl ProgressBarAuth {
    /// Construct new authentication prompt progress bar handler.
    pub(crate) fn new(bar_kind: ProgressBarKind) -> Self {
        Self { bar_kind }
    }
}

impl Prompter for ProgressBarAuth {
    fn prompt_username_password(
        &mut self,
        url: &str,
        _git_config: &git2::Config,
    ) -> Option<(String, String)> {
        let prompt = || -> Option<(String, String)> {
            log::info!("Authentication required for {url}");
            let username = Text::new("username").prompt().unwrap();
            let password = Password::new("password").without_confirmation().prompt().unwrap();
            Some((username, password))
        };

        match &self.bar_kind {
            ProgressBarKind::MultiBar(bar) => bar.suspend(prompt),
            ProgressBarKind::SingleBar(bar) => bar.suspend(prompt),
        }
    }

    fn prompt_password(
        &mut self,
        username: &str,
        url: &str,
        _git_config: &git2::Config,
    ) -> Option<String> {
        let prompt = || -> Option<String> {
            log::info!("Authentication required for {url} for user {username}");
            let password = Password::new("password").without_confirmation().prompt().unwrap();
            Some(password)
        };

        match &self.bar_kind {
            ProgressBarKind::MultiBar(bar) => bar.suspend(prompt),
            ProgressBarKind::SingleBar(bar) => bar.suspend(prompt),
        }
    }

    fn prompt_ssh_key_passphrase(
        &mut self,
        private_key_path: &Path,
        _git_config: &git2::Config,
    ) -> Option<String> {
        let prompt = || -> Option<String> {
            log::info!("Authentication required for {}", private_key_path.display());
            let password = Password::new("password").without_confirmation().prompt().unwrap();
            Some(password)
        };

        match &self.bar_kind {
            ProgressBarKind::MultiBar(bar) => bar.suspend(prompt),
            ProgressBarKind::SingleBar(bar) => bar.suspend(prompt),
        }
    }
}

/// Progress bar handler kind.
#[derive(Clone)]
pub(crate) enum ProgressBarKind {
    /// Need to handle only one progress bar.
    SingleBar(ProgressBar),

    /// Need to handle more than one progress bar.
    MultiBar(MultiProgress),
}

/// Sparse checkout manipulation.
#[derive(Debug, Default, Clone)]
pub(crate) struct SparseManip {
    sparse_path: PathBuf,
    rules: Vec<String>,
}

impl SparseManip {
    /// Construct new empty sparse checkout manipulator.
    pub(crate) fn new() -> Self {
        SparseManip::default()
    }

    /// Set expected path to sparse file.
    pub(crate) fn set_sparse_path(&mut self, path: &Path, kind: &RepoKind) {
        self.sparse_path = match kind {
            RepoKind::Normal => path.join(".git/info/sparse_checkout"),
            RepoKind::Bare | RepoKind::BareAlias(_) => path.join("info/sparse-checkout"),
        };
    }

    /// Add list of unwanted files to exclude from sparse checkout.
    pub(crate) fn add_unwanted(&mut self, unwanted: impl IntoIterator<Item = impl Into<String>>) {
        let mut vec = Vec::new();
        vec.extend(unwanted.into_iter().map(Into::into));
        self.rules = vec;
    }

    /// Write sparsity rules to sparse checkout excluding unwanted files.
    ///
    /// # Errors
    ///
    /// Will fail if sparsity rules cannot be written to sparse file for whatever reason.
    pub(crate) fn exclude_unwanted(&self) -> Result<()> {
        let excludes: String = self.rules.iter().fold(String::new(), |mut acc, u| {
            writeln!(&mut acc, "!{u}").unwrap();
            acc
        });

        let mut file = File::create(&self.sparse_path)?;
        file.write_all(format!("/*\n{excludes}").as_bytes())?;

        Ok(())
    }

    /// Write sparsity rules to include all files of index.
    ///
    /// # Errors
    ///
    /// Will fail if sparsity rules cannot be written to sparse file for whatever reason.
    pub(crate) fn include_all(&self) -> Result<()> {
        let mut file = File::create(&self.sparse_path)?;
        file.write_all("/*".as_bytes())?;
        Ok(())
    }

    /// Write sparsity rules to exclude the entire index from worktree.
    ///
    /// # Errors
    ///
    /// Will fail if sparsity rules cannot be written to sparse file for whatever reason.
    pub fn exclude_all(&self) -> Result<()> {
        let mut file = File::create(&self.sparse_path)?;
        file.write_all("".as_bytes())?;
        Ok(())
    }
}
