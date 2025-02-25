// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Version control handling.
//!
//! This module provides basic utilities for managing repository data through target version control
//! system. Currently, Git is the main VCS tool targted by OCD for managing repository data.

use crate::config::{Cluster, Layout, Node};

use anyhow::{anyhow, Context, Result};
use futures::{stream, TryStreamExt};
use git2::{build::RepoBuilder, FetchOptions, Progress, RemoteCallbacks, Repository};
use git2_credentials::{ui4dialoguer::CredentialUI4Dialoguer, CredentialHandler};
use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    fmt::Write as FmtWrite,
    fs::File,
    io::Write as IoWrite,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
};

/// Root repository handler.
///
/// The root repository is a a special bare-alias Git repository. It represents the root of a given
/// cluster. It is responsible for containing the configuration data that defines the cluster
/// itself. The alias worktree points to `$XDG_CONFIG_HOME/ocd` by default, but can be changed to
/// a different path based on the [`Cluster::worktree`] setting in the special `cluster.toml`
/// configuration file that the repository tracks.
///
/// ## Invariants
///
/// - There can only be one root repository for any given cluster.
/// - Root repository will always exist in `$XDG_DATA_HOME/ocd/root`.
///
/// [`Cluster::worktree`]: crate::config::Cluster::worktree
#[derive(Debug, Default, Clone)]
pub struct RootRepo {
    path: PathBuf,
    kind: RepoKind,
    sparsity: SparseManip,
}

impl RootRepo {
    /// Construct new root repository by cloning it from URL.
    ///
    /// Will clone existing root repository from URL provided, while showing a pretty progress bar
    /// of the clone progress itself.
    ///
    /// ## Errors
    ///
    /// - Will fail if clone itself fails.
    /// - Will fail if cloned root repository does not contain a `cluster.toml` file to parse.
    pub fn new_clone(url: impl AsRef<str>, layout: &Layout) -> Result<Self> {
        let style = ProgressStyle::with_template(
            "{wide_msg} {total_bytes}  {bytes_per_sec:>10}  {elapsed_precise}  [{bar:<50.blue}]",
        )?
        .progress_chars("-Cco ");
        let bar = ProgressBar::no_length()
            .with_message(format!("CLUSTER -- {}", url.as_ref()))
            .with_finish(ProgressFinish::AndLeave)
            .with_style(style);

        let path = layout.data_dir().join("root");
        let kind = RepoKind::Bare;
        let sparsity = SparseManip::new(&path, &kind);

        let repo = git_clone(url.as_ref(), &path, &kind, bar)?;
        let mut config = repo.config()?;
        config.set_str("status.showUntrackedFiles", "no")?;
        config.set_str("core.sparseCheckout", "true")?;

        let mut root = Self {
            path,
            kind,
            sparsity,
        };
        let cluster = root.get_cluster()?;
        let worktree = cluster
            .worktree
            .unwrap_or(layout.config_dir().to_path_buf());
        root.kind = RepoKind::BareAlias(AliasDir::new(worktree));
        root.sparsity.add_rules(cluster.excludes.iter().flatten());

        Ok(root)
    }

    /// Get contents of cluster configuration file.
    ///
    /// Will extract the `cluster.toml` file in root repository, and return it in deserialized form
    /// for further manipulation.
    ///
    /// ## Errors
    ///
    /// - Will fail if `cluster.toml` file does not exist in root repository.
    /// - Will fail if `cluster.toml` contains invalid formatting.
    pub fn get_cluster(&self) -> Result<Cluster> {
        self.git_bin(["cat-file", "-p", "@:cluster.toml"])?
            .replace("stdout:", "")
            .parse::<Cluster>()
    }

    /// Deploy contents of root repository to alias worktree.
    ///
    /// ## Errors
    ///
    /// - Will fail if root repository does not exist.
    pub fn deploy(&self) -> Result<()> {
        self.sparsity.exclude_unwanted()?;
        let output = self.git_bin(["checkout"])?;
        if !output.is_empty() {
            log::info!("deploy root: {output}");
        }

        Ok(())
    }

    /// Call Git binary on root repository.
    ///
    /// Will perform system call to user's Git binary to execute a set of arguments on the root
    /// repository itself.
    ///
    /// ## Errors
    ///
    /// - Will fail if Git is not installed.
    /// - Will fail if caller passed invalid arguments to Git.
    /// - Will fail if root repository does not exist.
    pub fn git_bin(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<String> {
        syscall_git(&self.path, &self.kind, args)
    }
}

/// Node repository handler.
///
/// A node repository is an existing entry to a given [`Cluster`]. Node repositories house any
/// configuration files/data that user wants to use for deployment. These repositories can be
/// normal or bare-alias, and the amount that can exist inside of a cluster is unbounded.
///
/// [`Cluster`]: crate::config::Cluster
#[derive(Debug, Default, Clone)]
pub struct NodeRepo {
    path: PathBuf,
    url: String,
    kind: RepoKind,
    sparsity: SparseManip,
}

impl NodeRepo {
    /// Construct new node repository handler from [`Node`].
    ///
    /// Extracts required information to handle node repository from [`Node`] parameter. Path of
    /// node repository will be set in `$XDG_DATA_HOME/ocd` using provided name for [`Node`].
    ///
    /// [`Node`]: crate::config::Node
    pub fn from_node(name: impl AsRef<str>, node: &Node, layout: &Layout) -> Self {
        let path = layout.data_dir().join(name.as_ref());
        let url = node.url.clone();
        let kind = node.repo_kind(layout);
        let mut sparsity = SparseManip::new(&path, &kind);
        sparsity.add_rules(node.excludes.iter().flatten());

        Self {
            path,
            url,
            kind,
            sparsity,
        }
    }
}

/// Handler for the cloning of multiple [`NodeRepo`].
pub struct NodeMultiClone {
    repos: Vec<NodeRepo>,
    bars: Arc<MultiProgress>,
}

impl NodeMultiClone {
    /// Construct a list of [`NodeRepo`] to clone based on [`Node`] entries in [`Cluster`].
    ///
    /// [`Cluster`]: crate::config::Cluster
    /// [`Node`]: crate::config::Node
    pub fn new(cluster: &Cluster, layout: &Layout) -> Self {
        let repos: Vec<NodeRepo> = cluster
            .node
            .iter()
            .map(|(name, node)| NodeRepo::from_node(name, node, layout))
            .collect();
        let bars = Arc::new(MultiProgress::new());
        Self { repos, bars }
    }

    /// Clone all [`NodeRepo`] entries.
    ///
    /// Asynchronously clones each [`NodeRepo`] entry, showing a set of pretty progress bars for
    /// each progressive clone being done. Caller can control the number of clones being performed
    /// such that [`None`] will try to saturate all CPU cores.
    ///
    /// ## Errors
    ///
    /// - Will fail if any clone task fails.
    pub async fn clone_all(self, jobs: Option<usize>) -> Result<()> {
        // INVARIANT: Catch all clone task failures by converting vec of `NodeRepo` into vec of
        // `Result<NodeRepo>` for catching through `?`, i.e., `Try`.
        stream::iter(self.repos.into_iter().map(Ok::<NodeRepo, anyhow::Error>))
            .try_for_each_concurrent(jobs, |node| {
                let bars = self.bars.clone();
                let node = node.clone();
                async move {
                    let _ = tokio::spawn(clone_node_task(node, bars)).await?;
                    Ok(())
                }
            })
            .await
    }
}

async fn clone_node_task(node: NodeRepo, bars: Arc<MultiProgress>) -> Result<()> {
    let style = ProgressStyle::with_template(
        "{wide_msg} {total_bytes}  {bytes_per_sec:>10}  {elapsed_precise}  [{bar:<50.yellow}]",
    )?
    .progress_chars("-Cco ");
    let bar = bars.add(
        ProgressBar::no_length()
            .with_message(format!("NODE -- {}", node.url))
            .with_finish(ProgressFinish::AndLeave)
            .with_style(style),
    );

    let repo = git_clone(&node.url, &node.path, &node.kind, bar)?;
    let mut config = repo.config()?;
    config.set_str("status.showUntrackedFiles", "no")?;
    config.set_str("core.sparseCheckout", "true")?;

    Ok(())
}

fn git_clone(
    url: impl AsRef<str>,
    path: impl AsRef<Path>,
    kind: &RepoKind,
    bar: ProgressBar,
) -> Result<Repository> {
    let state = RefCell::new(CloneState {
        progress: None,
        bar,
    });

    let mut rc = RemoteCallbacks::new();

    // INVARIANT: Verify credentials of user when needed. Make calls to whatever prompt software
    // the user established as their default to obtain required credentials.
    let cfg = git2::Config::open_default()?;
    let mut ch = CredentialHandler::new_with_ui(cfg, Box::new(CredentialUI4Dialoguer {}));
    rc.credentials(move |url, username, allowed| ch.try_next_credential(url, username, allowed));

    rc.transfer_progress(|stats| {
        let mut state = state.borrow_mut();
        state.progress = Some(stats.to_owned());
        clone_progress(&mut state);
        true
    });

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(rc);

    let repo = RepoBuilder::new()
        .bare(kind.is_bare())
        .fetch_options(fo)
        .clone(url.as_ref(), path.as_ref())?;

    // INVARIANT: Finish current progress bar with current message to prevent visual glitches.
    let state = state.borrow_mut();
    state.bar.finish();

    Ok(repo)
}

struct CloneState {
    progress: Option<Progress<'static>>,
    bar: ProgressBar,
}

fn clone_progress(state: &mut CloneState) {
    let stats = state.progress.as_ref().unwrap();
    let total_size: u64 = stats.total_objects().try_into().unwrap();
    let current: u64 = stats.received_objects().try_into().unwrap();
    state.bar.set_length(total_size);
    state.bar.set_position(current);
}

/// Determine kind of repository to use.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub enum RepoKind {
    /// Normal repository where its gitdir and worktree point to the same path.
    #[default]
    Normal,

    /// Normal bare repository without any worktree.
    Bare,

    /// Bare repository that uses external directory as an alias of a worktree.
    BareAlias(AliasDir),
}

impl RepoKind {
    pub(crate) fn is_bare(&self) -> bool {
        match self {
            RepoKind::Normal => false,
            RepoKind::Bare | RepoKind::BareAlias(_) => true,
        }
    }
}

/// Directory to act as alias of worktree.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct AliasDir(pub PathBuf);

impl AliasDir {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn to_os_string(&self) -> OsString {
        OsString::from(self.0.to_string_lossy().into_owned())
    }
}

#[derive(Debug, Clone, Default)]
struct SparseManip {
    sparse_file: PathBuf,
    rules: Vec<String>,
}

impl SparseManip {
    fn new(path: &Path, kind: &RepoKind) -> Self {
        let sparse_file = match kind {
            RepoKind::Normal => path.join(".git/info/sparse_checkout"),
            RepoKind::Bare | RepoKind::BareAlias(_) => path.join("info/sparse-checkout"),
        };

        Self {
            sparse_file,
            ..Default::default()
        }
    }

    fn add_rules(&mut self, unwanted: impl IntoIterator<Item = impl Into<String>>) {
        let mut vec = Vec::new();
        vec.extend(unwanted.into_iter().map(Into::into));
        self.rules = vec;
    }

    fn exclude_unwanted(&self) -> Result<()> {
        let excludes: String = self.rules.iter().fold(String::new(), |mut acc, u| {
            writeln!(&mut acc, "!{}", u).unwrap();
            acc
        });

        let mut file = File::create(&self.sparse_file)?;
        file.write_all(format!("/*\n{excludes}").as_bytes())?;

        Ok(())
    }
}

fn syscall_git(
    path: impl AsRef<Path>,
    kind: &RepoKind,
    args: impl IntoIterator<Item = impl Into<OsString>>,
) -> Result<String> {
    let gitdir: OsString = if kind == &RepoKind::Normal {
        path.as_ref().join(".git")
    } else {
        path.as_ref().to_path_buf()
    }
    .to_string_lossy()
    .into_owned()
    .into();

    let path_args: Vec<OsString> = match kind {
        RepoKind::Normal => vec!["--git-dir".into(), gitdir],
        RepoKind::Bare => vec!["--git-dir".into(), gitdir],
        RepoKind::BareAlias(alias) => vec![
            "--git-dir".into(),
            gitdir,
            "--work-tree".into(),
            alias.to_os_string(),
        ],
    };

    let mut bin_args: Vec<OsString> = Vec::new();
    bin_args.extend(path_args);
    bin_args.extend(args.into_iter().map(Into::into));

    syscall("git", bin_args)
}

fn syscall(
    cmd: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<String> {
    let args: Vec<OsString> = args
        .into_iter()
        .map(|s| s.as_ref().to_os_string())
        .collect();
    log::debug!("Syscall: {:?} {:?}", cmd.as_ref(), args);

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
        .map(|s| s.to_string())
        .unwrap_or(message);

    message
}
