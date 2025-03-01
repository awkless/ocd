// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::config::{Cluster, Layout, Node};

use anyhow::{anyhow, Context, Result};
use auth_git2::{GitAuthenticator, Prompter};
use beau_collector::BeauCollector as _;
use futures::{stream, StreamExt};
use git2::{build::RepoBuilder, FetchOptions, RemoteCallbacks};
use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use inquire::{Password, Text};
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
        let style = ProgressStyle::with_template(
            "{wide_msg} {bytes:>10} /{total_bytes:>10}  {bytes_per_sec:>10.magenta}  {elapsed_precise:.green}  [{bar:<50.yellow/blue}]",
        ).unwrap()
        .progress_chars("-Cco.");
        let bar = ProgressBar::no_length()
            .with_message(url.as_ref().to_string())
            .with_style(style);
        bar.enable_steady_tick(Duration::from_millis(100));

        let git = GitWrapper::new("root", layout)
            .with_url(url.as_ref())
            .with_kind(RepoKind::Bare);
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

    pub fn get_cluster(&self) -> Result<Cluster> {
        self.git
            .syscall(["cat-file", "-p", "@:cluster.toml"])?
            .replace("stdout:", "")
            .parse::<Cluster>()
    }

    pub fn deploy(&self) -> Result<()> {
        self.git.deploy()
    }

    pub fn nuke(&self, layout: &Layout) -> Result<()> {
        log::info!("Clear out cluster");
        remove_dir_all(layout.config_dir())?;
        remove_dir_all(layout.data_dir())?;
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
                GitWrapper::from_node(name, node, layout)
                    .with_auth_prompt(Git2AuthPrompt::new(multi_bar.clone()))
            })
            .collect();
        Self { repos, multi_bar }
    }

    pub async fn clone_all(self, jobs: Option<usize>) -> Result<()> {
        let mut bars = Vec::new();
        let results = Arc::new(Mutex::new(Vec::new()));

        stream::iter(self.repos)
            .for_each_concurrent(jobs, |repo| {
                let style = ProgressStyle::with_template(
                    "{wide_msg} {bytes:>10} /{total_bytes:>10}  {bytes_per_sec:>10.magenta}  {elapsed_precise:.green}  [{bar:<50.yellow/blue}]",
                ).unwrap()
                .progress_chars("-Cco.");
                let bar = self.multi_bar.add(
                    ProgressBar::no_length()
                        .with_message(repo.url.clone())
                        .with_finish(ProgressFinish::AndLeave)
                        .with_style(style),
                );
                bar.enable_steady_tick(Duration::from_millis(100));

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

    pub fn from_node(repo_name: &str, node: &Node, layout: &Layout) -> Self {
        Self::new(repo_name, layout)
            .with_url(&node.url)
            .with_kind(node.repo_kind(layout))
    }

    pub fn clone_with_progress(&self, bar: &ProgressBar) -> Result<()> {
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

        let mut config = repo.config()?;
        config.set_str("status.showUntrackedFiles", "no")?;
        config.set_str("core.sparseCheckout", "true")?;

        Ok(())
    }

    pub fn deploy(&self) -> Result<()> {
        self.sparsity.exclude_unwanted()?;
        let output = self.syscall(["checkout"])?;
        if !output.is_empty() {
            log::info!("deploy root: {output}");
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

#[derive(Clone)]
struct Git2AuthPrompt {
    multi_bar: MultiProgress,
}

impl Git2AuthPrompt {
    pub fn new(multi_bar: MultiProgress) -> Self {
        Self { multi_bar }
    }
}

impl Prompter for Git2AuthPrompt {
    fn prompt_username_password(
        &mut self,
        url: &str,
        _git_config: &git2::Config,
    ) -> Option<(String, String)> {
        self.multi_bar.suspend(|| {
            log::info!("Authentication required for {url}");
            let username = Text::new("username").prompt().unwrap();
            let password = Password::new("password")
                .without_confirmation()
                .prompt()
                .unwrap();
            Some((username, password))
        })
    }

    fn prompt_password(
        &mut self,
        username: &str,
        url: &str,
        _git_config: &git2::Config,
    ) -> Option<String> {
        self.multi_bar.suspend(|| {
            log::info!("Authentication required for {url} for user {username}");
            let password = Password::new("password")
                .without_confirmation()
                .prompt()
                .unwrap();
            Some(password)
        })
    }

    fn prompt_ssh_key_passphrase(
        &mut self,
        private_key_path: &Path,
        _git_config: &git2::Config,
    ) -> Option<String> {
        self.multi_bar.suspend(|| {
            log::info!("Authentication required for {}", private_key_path.display());
            let password = Password::new("password")
                .without_confirmation()
                .prompt()
                .unwrap();
            Some(password)
        })
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
