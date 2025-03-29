// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

//! Version control system management.
//!
//! This module provides APIs to manipulate version control repository data. Currently, OCD mainly
//! targets Git as the primary VCS of choice. Thus, the code here makes it easy to manipulate Git
//! repository data in a fashion that makes sense to OCD's codebase.

use crate::{
    cluster::{Cluster, Node},
    fs::DirLayout,
};

use anyhow::{anyhow, Context, Result};
use auth_git2::{GitAuthenticator, Prompter};
use beau_collector::BeauCollector as _;
use futures::{stream, StreamExt};
use git2::{build::RepoBuilder, FetchOptions, RemoteCallbacks};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::{Password, Text};
use std::{
    ffi::{OsStr, OsString},
    fmt::Write as FmtWrite,
    fs::File,
    io::Write as IoWrite,
    path::{Path, PathBuf},
    process::{Command, Output},
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
    /// ## Errors
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

    /// Extract cluster configuration file.
    ///
    /// ## Errors
    ///
    /// Will fail if repository does not contain a cluster configuration file.
    pub fn get_cluster(&self) -> Result<Cluster> {
        self.0
            .bincall(["cat-file", "-p", "@:cluster.toml"])?
            .replace("stdout:", "")
            .parse::<Cluster>()
    }

    /// Deploy root repository to worktree alias.
    ///
    /// Excludes unwanted files from deployment.
    ///
    /// ## Errors
    ///
    /// Will fail if root repository cannot be deployed to worktree alias, or
    /// sparse checkout fails to exclude unwanted files.
    pub fn deploy(&self) -> Result<()> {
        self.0.deploy()
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
    /// ## Errors
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

    /// Clone repository with progress bar output.
    ///
    /// Performs Git clone without system call, with a progress bar to interactively show how long
    /// the clone is taking for the user. Method may prompt user for credentials if it cannot be
    /// automatically determined. This prompt may occur through external program or through the
    /// current terminal the user is running OCD on.
    ///
    /// ## Errors
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

    /// Deploy repository to target worktree alias.
    ///
    /// Will exclude unwanted files through sparse checkout.
    ///
    /// ## Errors
    ///
    /// Will fail if Git checkout fails, or writing sparsity rules fails.
    pub fn deploy(&self) -> Result<()> {
        self.sparsity.exclude_unwanted()?;
        let output = self.bincall(["checkout"])?;
        if !output.is_empty() {
            log::info!("deploy {}:\n{output}", self.path.display());
        }

        Ok(())
    }

    /// Make system call to Git binary.
    ///
    /// Returns data sent to stdout and stderr as a loggable string.
    ///
    /// ## Errors
    ///
    /// Will fail if Git binary cannot be found, or provided arguments are invalid to Git binary
    /// itself.
    pub fn bincall(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<String> {
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

        syscall("git", bin_args)
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
    /// ## Errors
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
}

fn syscall(
    cmd: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<String> {
    let args: Vec<OsString> = args.into_iter().map(|s| s.as_ref().to_os_string()).collect();
    let output = Command::new(cmd.as_ref())
        .args(args)
        .output()
        .with_context(|| format!("Failed to call {:?}", cmd.as_ref()))?;
    let message = format_cmd_output(&output);
    if !output.status.success() {
        return Err(anyhow!("{:?} failed\n{message}", cmd.as_ref()));
    }
    Ok(message)
}

fn format_cmd_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(output.stdout.as_slice()).into_owned();
    let stderr = String::from_utf8_lossy(output.stderr.as_slice()).into_owned();
    let mut message = String::new();

    if !stdout.is_empty() {
        message.push_str(format!("stdout: {stdout}").as_str());
    }

    if !stderr.is_empty() {
        message.push_str(format!("stderr: {stderr}").as_str());
    }

    let message = message
        .strip_suffix("\r\n")
        .or(message.strip_suffix('\n'))
        .map(ToString::to_string)
        .unwrap_or(message);

    message
}
