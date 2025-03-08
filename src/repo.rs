// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

mod auth;

use crate::{
    config::{Cluster, Layout, Node},
    repo::auth::{ProgressBarAuth, ProgressBarKind},
};

use anyhow::{anyhow, Context, Result};
use auth_git2::{GitAuthenticator, Prompter};
use beau_collector::BeauCollector as _;
use futures::{stream, StreamExt};
use git2::{build::RepoBuilder, FetchOptions, RemoteCallbacks};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{
    ffi::{OsStr, OsString},
    fmt::Write as FmtWrite,
    fs::{remove_dir_all, File},
    io::Write as IoWrite,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct RootRepo {
    git: GitWrapper,
}

impl RootRepo {
    pub fn new_clone(url: impl AsRef<str>, layout: &Layout) -> Result<Self> {
        let bar = ProgressBar::no_length();
        let git = GitWrapper::new("root", layout)
            .with_url(url.as_ref())
            .with_kind(RepoKind::Bare)
            .with_auth_prompt(ProgressBarAuth::new(ProgressBarKind::SingleBar(
                bar.clone(),
            )));
        git.clone_with_progress(&bar)?;
        bar.finish_and_clear();

        let mut root = Self { git };
        let cluster = root.get_cluster()?;
        let worktree = cluster
            .worktree
            .unwrap_or(layout.config_dir().to_path_buf());
        root.git.kind = RepoKind::BareAlias(AliasDir::new(worktree));
        root.git
            .sparsity
            .add_unwanted(cluster.excludes.iter().flatten());

        Ok(root)
    }

    pub fn from_cluster(cluster: &Cluster, layout: &Layout) -> Self {
        let worktree = cluster
            .worktree
            .as_ref()
            .map(|p| p.as_ref())
            .unwrap_or(layout.config_dir());
        let git = GitWrapper::new("root", layout)
            .with_kind(RepoKind::BareAlias(AliasDir::new(worktree)))
            .with_excludes(cluster.excludes.iter().flatten());
        Self { git }
    }

    pub fn get_cluster(&self) -> Result<Cluster> {
        self.git
            .syscall(["cat-file", "-p", "@:cluster.toml"])?
            .replace("stdout:", "")
            .parse::<Cluster>()
    }

    pub fn deploy(&self) -> Result<()> {
        self.git.deploy()
    }

    pub fn nuke_cluster(&self, layout: &Layout) -> Result<()> {
        log::info!("Clear out cluster");
        self.git.sparsity.exclude_all()?;

        remove_dir_all(layout.data_dir())?;

        Ok(())
    }

    pub fn git_bin(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<()> {
        let output = self.git.syscall(args)?;
        if !output.is_empty() {
            log::info!("{}\n{output}", self.git.path.display());
        }

        Ok(())
    }
}

pub struct NodeRepo {
    git: GitWrapper,
}

impl NodeRepo {
    pub fn from_node(repo_name: &str, node: &Node, layout: &Layout) -> Self {
        let git = GitWrapper::from_node(repo_name, node, layout);
        Self { git }
    }

    pub fn deploy(&self) -> Result<()> {
        self.git.deploy()
    }

    pub fn git_bin(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<()> {
        let output = self.git.syscall(args)?;
        if !output.is_empty() {
            log::info!("{}\n{output}", self.git.path.display());
        }

        Ok(())
    }
}

pub struct MultiClone {
    repos: Vec<GitWrapper>,
    multi_bar: MultiProgress,
}

impl MultiClone {
    pub fn new(cluster: &Cluster, layout: &Layout) -> Self {
        let multi_bar = MultiProgress::new();
        let repos: Vec<GitWrapper> = cluster
            .node
            .iter()
            .map(|(name, node)| {
                GitWrapper::from_node(name, node, layout).with_auth_prompt(ProgressBarAuth::new(
                    ProgressBarKind::MultiBar(multi_bar.clone()),
                ))
            })
            .collect();
        Self { repos, multi_bar }
    }

    pub async fn clone_all(self, jobs: Option<usize>) -> Result<()> {
        let mut bars = Vec::new();
        let results = Arc::new(Mutex::new(Vec::new()));

        stream::iter(self.repos)
            .for_each_concurrent(jobs, |repo| {
                let bar = self.multi_bar.add(ProgressBar::no_length());

                bars.push(bar.clone());
                let results = results.clone();

                async move {
                    let result = tokio::spawn(Self::clone_task(repo, bar)).await;
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

    async fn clone_task(repo: GitWrapper, bar: ProgressBar) -> Result<()> {
        repo.clone_with_progress(&bar)
            .with_context(|| format!("Failed to clone {}", &repo.url))?;
        bar.finish();
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct GitWrapper {
    path: PathBuf,
    kind: RepoKind,
    url: String,
    auth: GitAuthenticator,
    sparsity: SparseManip,
}

impl GitWrapper {
    pub fn new(repo_name: &str, layout: &Layout) -> Self {
        let path = layout.data_dir().join(repo_name);

        Self {
            path,
            ..Default::default()
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    pub fn with_kind(mut self, kind: RepoKind) -> Self {
        self.kind = kind;
        self.sparsity
            .set_sparse_path(self.path.as_ref(), &self.kind);
        self
    }

    pub fn with_auth_prompt(mut self, prompter: impl Prompter + Clone + 'static) -> Self {
        self.auth = self.auth.set_prompter(prompter);
        self
    }

    pub fn with_excludes(mut self, unwanted: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.sparsity.add_unwanted(unwanted);
        self
    }

    pub fn from_node(repo_name: &str, node: &Node, layout: &Layout) -> Self {
        Self::new(repo_name, layout)
            .with_url(&node.url)
            .with_kind(node.repo_kind(layout))
            .with_excludes(node.excludes.iter().flatten())
    }

    pub fn clone_with_progress(&self, bar: &ProgressBar) -> Result<()> {
        let style = ProgressStyle::with_template(
            "{wide_msg} {bytes:>10} /{total_bytes:>10}  {bytes_per_sec:>10.magenta}  {elapsed_precise:.green}  [{bar:<50.yellow/blue}]",
        ).unwrap()
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
            .clone(self.url.as_ref(), self.path.as_ref())?;

        if matches!(self.kind, RepoKind::BareAlias(..) | RepoKind::Bare) {
            let mut config = repo.config()?;
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        Ok(())
    }

    pub fn deploy(&self) -> Result<()> {
        if !self.path.exists() {
            log::warn!("Must clone {} before deployment", &self.url);
            let bar = ProgressBar::no_length();
            self.clone_with_progress(&bar)?;
            bar.finish_and_clear();
        }

        self.sparsity.exclude_unwanted()?;
        let output = self.syscall(["checkout"])?;
        if !output.is_empty() {
            log::info!("deploy {}:\n{output}", self.path.display());
        }

        Ok(())
    }

    pub fn syscall(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<String> {
        let gitdir: OsString = if self.kind == RepoKind::Normal {
            self.path.join(".git")
        } else {
            self.path.clone()
        }
        .to_string_lossy()
        .into_owned()
        .into();

        let path_args: Vec<OsString> = match &self.kind {
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
}

#[derive(Debug, Default, Clone)]
struct SparseManip {
    sparse_path: PathBuf,
    rules: Vec<String>,
}

impl SparseManip {
    fn new() -> Self {
        Default::default()
    }

    fn set_sparse_path(&mut self, path: &Path, kind: &RepoKind) {
        self.sparse_path = match kind {
            RepoKind::Normal => path.join(".git/info/sparse_checkout"),
            RepoKind::Bare | RepoKind::BareAlias(_) => path.join("info/sparse-checkout"),
        };
    }

    fn add_unwanted(&mut self, unwanted: impl IntoIterator<Item = impl Into<String>>) {
        let mut vec = Vec::new();
        vec.extend(unwanted.into_iter().map(Into::into));
        self.rules = vec;
    }

    fn exclude_unwanted(&self) -> Result<()> {
        let excludes: String = self.rules.iter().fold(String::new(), |mut acc, u| {
            writeln!(&mut acc, "!{}", u).unwrap();
            acc
        });

        let mut file = File::create(&self.sparse_path)?;
        file.write_all(format!("/*\n{excludes}").as_bytes())?;

        Ok(())
    }

    fn exclude_all(&self) -> Result<()> {
        let mut file = File::create(&self.sparse_path)?;
        file.write_all("".as_bytes())?;
        Ok(())
    }
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

    pub(crate) fn unwrap_alias_worktree(&self) -> Option<&Path> {
        match self {
            RepoKind::Normal | RepoKind::Bare => None,
            RepoKind::BareAlias(alias) => Some(alias.0.as_ref()),
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
