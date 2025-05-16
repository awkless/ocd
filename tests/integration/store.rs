// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use ocd::store::*;
use simple_txtar::Archive;
use crate::{GitKind, GitFixture};

use anyhow::Result;
use pretty_assertions::assert_eq as pretty_assert_eq;
use sealed_test::prelude::*;
use simple_test_case::dir_cases;
use run_script::run_script;

#[dir_cases("tests/fixtures/root_new_open")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn root_new_open(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/root", GitKind::Bare)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let root = Root::new_open()?;
    assert!(root.is_deployed(DeployState::WithExcluded)?);

    Ok(())
}
