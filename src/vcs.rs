// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Version control handling.
//!
//! This module provides basic utilities for managing repository data through target version control
//! system. Currently, Git is the main VCS tool targted by OCD for managing repository data.

use anyhow::{anyhow, Context, Result};
use git2::{build::RepoBuilder, FetchOptions, Progress, RemoteCallbacks, Repository};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Perform Git clone.
///
/// Provides status information of the Git clone being done.
///
/// # Errors
///
/// Will fail if any step of the cloning process fails for whatever reason.
pub fn git_clone(
    path: impl AsRef<Path>,
    url: impl AsRef<str>,
    bare: bool,
    cb: impl FnMut(Progress) -> bool,
) -> Result<()> {
    let mut rc = RemoteCallbacks::new();
    rc.transfer_progress(cb);

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(rc);

    log::info!("Clone {}", url.as_ref());
    RepoBuilder::new()
        .bare(bare)
        .fetch_options(fo)
        .clone(url.as_ref(), path.as_ref())?;

    Ok(())
}

pub fn git_init(path: impl AsRef<Path>, kind: &RepoKind) -> Result<()> {
    log::info!("Initialize new repository at {}", path.as_ref().display());
    let repo = match kind {
        RepoKind::Normal => Repository::init(path.as_ref())?,
        RepoKind::BareAlias(_) => Repository::init_bare(path.as_ref())?,
    };

    if matches!(kind, RepoKind::BareAlias(..)) {
        let mut config = repo.config()?;
        config.set_str("status.showUntrackedFiles", "no")?;
        config.set_str("core.sparseCheckout", "true")?;
    }

    Ok(())
}

/// Run user's Git command.
///
/// Will execute given Git command on target repository. Will execute on normal
/// or bare-alias repositories based on `kind` argument.
///
/// # Errors
///
/// Will if Git command does not exist on user's system, or Git itself fails
/// for whatever reason.
pub fn syscall_git(
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

    let worktree = match kind {
        RepoKind::Normal => gitdir.clone(),
        RepoKind::BareAlias(alias) => alias.to_os_string(),
    };
    let path_args = vec!["--git-dir".into(), gitdir, "--work-tree".into(), worktree];

    let mut bin_args: Vec<OsString> = Vec::new();
    bin_args.extend(path_args.into_iter().map(Into::into));
    bin_args.extend(args.into_iter().map(Into::into));

    syscall("git", bin_args)
}

/// Determine kind of repository to use.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub enum RepoKind {
    /// Normal repository where its gitdir and worktree point to the same path.
    #[default]
    Normal,

    /// Bare repository that uses external directory as an alias of a worktree.
    BareAlias(AliasDir),
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

/// Call external binary.
///
/// Makes call to target binary on user's machine with a set of arguments to pass to it. Returns
/// string of any output the binary writes to stdout and stderr.
///
/// # Errors
///
/// Will fail if target binary does not exist, or binary itself fails due to invalid arguments or
/// some other reason. If binary itself fails, then any output written to stdout and stderr will be
/// included with error output.
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
        message.push_str(format!("Stdout: {stdout}").as_str());
    }

    if !stderr.is_empty() {
        message.push_str(format!("Stderr: {stderr}").as_str());
    }

    let message = message
        .strip_suffix("\r\n")
        .or(message.strip_suffix('\n'))
        .map(|s| s.to_string())
        .unwrap_or(message);

    message
}
