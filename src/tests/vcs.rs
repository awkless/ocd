// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use super::{RepoFixture, RepoFixtureKind};

use crate::vcs::*;

use anyhow::{anyhow, Result};
use pretty_assertions::assert_eq;
use rstest::rstest;
use sealed_test::prelude::*;
use std::{ffi::OsString, path::Path};

fn setup_repos() -> Result<()> {
    let repo = RepoFixture::init("repos/foo", false)?;
    repo.write_blob_then_commit("hello.txt", "hello there")?;

    let repo = RepoFixture::init("repos/bar", true)?;
    repo.write_blob_then_commit(".dotfile", "some.config = 123")?;

    Ok(())
}

#[rstest]
#[case::normal("repos/foo", RepoKind::Normal)]
#[case::bare_alisa("repos/bar", RepoKind::BareAlias(AliasDir::new("./")))]
#[sealed_test]
fn test_git_init(#[case] path: impl AsRef<Path>, #[case] kind: RepoKind) -> Result<()> {
    git_init(path.as_ref(), &kind)?;
    assert!(path.as_ref().exists());
    Ok(())
}

#[rstest]
#[case::normal(
    "repos/foo",
    &RepoKind::Normal,
    ["show", "master:hello.txt"],
    Ok("Stdout: hello there".into()),
)]
#[case::bare_alias(
    "repos/bar",
    &RepoKind::BareAlias(AliasDir::new("./")),
    ["show", "master:.dotfile"],
    Ok("Stdout: some.config = 123".into()),
)]
#[case::invalid_repo_kind(
    "repos/bar",
    &RepoKind::Normal,
    ["fail"],
    Err(anyhow!("Should fail")),
)]
#[case::invalid_args(
    "repos/foo",
    &RepoKind::Normal,
    ["fail", "--snafu"],
    Err(anyhow!("Should fail")),
)]
#[sealed_test(before = setup_repos()?)]
fn test_syscall_git(
    #[case] path: impl AsRef<Path>,
    #[case] kind: &RepoKind,
    #[case] args: impl IntoIterator<Item = impl Into<OsString>>,
    #[case] expect: Result<String>,
) -> Result<()> {
    let result = syscall_git(path.as_ref(), kind, args);
    match expect {
        Ok(expect) => assert_eq!(result.unwrap(), expect.as_ref()),
        Err(_) => assert!(result.is_err()),
    };
    Ok(())
}
