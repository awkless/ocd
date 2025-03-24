// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

use anyhow::{anyhow, Context, Result};
use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
    process::{Command, Output},
};

#[derive(Debug, Default, Clone)]
pub struct Git {
    path: PathBuf,
    kind: RepoKind,
    url: String,
}

impl Git {
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

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub enum RepoKind {
    #[default]
    Normal,

    Bare,

    BareAlias(AliasDir),
}

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
