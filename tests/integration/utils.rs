// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use ocd::utils::{syscall_interactive, syscall_non_interactive};

use anyhow::{anyhow, Result};
use pretty_assertions::assert_eq;
use std::ffi::OsStr;

#[track_caller]
fn check_syscall_non_interactive(
    cmd: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    expect: Result<String>,
) {
    let result = syscall_non_interactive(cmd, args);
    match expect {
        Ok(message) => assert_eq!(result.unwrap(), message),
        Err(_) => assert!(result.is_err()),
    }
}

#[track_caller]
fn check_syscall_interactive(
    cmd: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    expect: Result<()>,
) {
    let result = syscall_interactive(cmd, args);
    match expect {
        Ok(_) => assert!(result.is_ok()),
        Err(_) => assert!(result.is_err()),
    }
}


#[test]
fn smoke_syscall_non_interactive() {
    check_syscall_non_interactive(
        "git",
        ["ls-files", "README.md"],
        Ok("stdout: README.md\n".into()),
    );
    check_syscall_non_interactive("not_found", ["fail"], Err(anyhow!("should fail")));
    check_syscall_non_interactive("cd", ["--bad-flag"], Err(anyhow!("should fail")));
}

#[test]
fn smoke_syscall_interactive() {
    check_syscall_interactive("git", ["ls-files", "README.md"], Ok(()));
    check_syscall_interactive("not_found", ["fail"], Err(anyhow!("should fail")));
    check_syscall_interactive("cd", ["--bad-flag"], Err(anyhow!("should fail")));
}
