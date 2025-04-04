// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT


use crate::utils::*;

use anyhow::{anyhow, Result};
use pretty_assertions::assert_eq;
use rstest::rstest;
use std::ffi::OsStr;

#[rstest]
#[case::match_all(["*"], ["foo", "bar", "baz"], ["foo", "bar", "baz"])]
#[case::match_single_glob(["*sh"], ["sh", "bash", "yash", "vim"], ["sh", "bash", "yash"])]
#[case::match_no_glob(["vim", "foo"], ["foo", "dwm", "bar", "vim"], ["vim", "foo"])]
#[case::no_match(["foo", "bar"], ["vim", "dwm", "sh"], Vec::<String>::new())]
fn smoke_glob_match(
    #[case] patterns: impl IntoIterator<Item = impl Into<String>>,
    #[case] entries: impl IntoIterator<Item = impl Into<String>>,
    #[case] expect: impl IntoIterator<Item = impl Into<String>>,
) {
    let mut expect = expect.into_iter().map(Into::into).collect::<Vec<String>>();
    let mut result = glob_match(patterns, entries);
    expect.sort();
    result.sort();
    assert_eq!(result, expect);
}

#[rstest]
#[case("git", ["ls-files", "README.md"], Ok("stdout: README.md\n".into()))]
#[case("not_found", ["fail"], Err(anyhow!("should fail")))]
#[case("cd", ["--bad-flag"], Err(anyhow!("should fail")))]
fn smoke_syscall_non_interactive(
    #[case] cmd: impl AsRef<OsStr>,
    #[case] args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    #[case] expect: Result<String>,
) {
    let result = syscall_non_interactive(cmd, args);
    match expect {
        Ok(message) => assert_eq!(result.unwrap(), message),
        Err(_) => assert!(result.is_err()),
    }
}

#[rstest]
#[case("git", ["ls-files", "foo"], Ok(()))]
#[case("not_found", ["fail"], Err(anyhow!("should fail")))]
#[case("cd", ["--bad-flag"], Err(anyhow!("should fail")))]
fn smoke_syscall_interactive(
    #[case] cmd: impl AsRef<OsStr>,
    #[case] args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    #[case] expect: Result<()>,
) {
    let result = syscall_interactive(cmd, args);
    match expect {
        Ok(_) => assert!(result.is_ok()),
        Err(_) => assert!(result.is_err()),
    }
}
