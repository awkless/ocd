// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::{
    model::{DeploymentKind, DirAlias, NodeEntry},
    store::{DeployAction, DeployState, Node, Root},
    tests::{GitFixture, GitKind},
    Result,
};

use run_script::run_script;
use sealed_test::prelude::*;
use simple_test_case::dir_cases;
use simple_txtar::Archive;
use std::path::PathBuf;

#[sealed_test(env = [("XDG_DATA_HOME", "store")])]
fn smoke_root_new_init() -> Result<()> {
    std::env::set_var("HOME", std::env::current_dir()?);
    let root = Root::new_init()?;
    assert!(root.path().exists());

    Ok(())
}

#[dir_cases("src/tests/fixture/root_new_open")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn smoke_root_new_open(_: &str, contents: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(contents);
    let git = GitFixture::new(".local/share/ocd/root", GitKind::Bare)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let root = Root::new_open()?;
    assert!(root.is_deployed(DeployState::WithoutExcluded)?);

    Ok(())
}

#[dir_cases("src/tests/fixture/root_new_clone")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn smoke_root_new_clone(_: &str, contents: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(contents);
    let git = GitFixture::new("forge/remote_root.git", GitKind::Bare)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }

    let root = Root::new_clone("forge/remote_root.git")?;
    assert!(root.path().exists());
    assert!(root.is_deployed(DeployState::WithoutExcluded)?);

    Ok(())
}

#[dir_cases("src/tests/fixture/root_nuke")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn smoke_root_nuke(_: &str, contents: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(contents);
    let fixture = txtar.get("root/cluster.toml").unwrap();
    GitFixture::new(".local/share/ocd/root", GitKind::Bare)?
        .stage_and_commit("cluster.toml", &fixture.content)?;
    let fixture = txtar.get("sh/.shrc").unwrap();
    GitFixture::new(".local/share/ocd/sh", GitKind::Bare)?
        .stage_and_commit(".shrc", &fixture.content)?;
    let fixture = txtar.get("vim/.vimrc").unwrap();
    GitFixture::new(".local/share/ocd/vim", GitKind::Bare)?
        .stage_and_commit(".vimrc", &fixture.content)?;
    let fixture = txtar.get("dwm/dwm.c").unwrap();
    GitFixture::new(".local/share/ocd/dwm", GitKind::Normal)?
        .stage_and_commit("dwm.c", &fixture.content)?;
    run_script!(&txtar.comment())?;

    let root = Root::new_open()?;
    root.nuke()?;

    assert!(!PathBuf::from(".config/ocd").exists());
    assert!(!PathBuf::from(".local/share/ocd/root").exists());

    Ok(())
}

#[sealed_test(env = [("XDG_DATA_HOME", "store")])]
fn smoke_node_new_init() -> Result<()> {
    std::env::set_var("HOME", std::env::current_dir()?);

    let entry = NodeEntry { deployment: DeploymentKind::Normal, ..Default::default() };
    let node = Node::new_init("dwm", &entry)?;
    assert!(node.path().exists());
    assert!(!node.is_bare_alias());

    let entry = NodeEntry {
        deployment: DeploymentKind::BareAlias(DirAlias::new("some/path")),
        ..Default::default()
    };
    let node = Node::new_init("vim", &entry)?;
    assert!(node.path().exists());
    assert!(node.is_bare_alias());

    Ok(())
}

#[dir_cases("src/tests/fixture/node_new_open")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn smoke_node_new_open(_: &str, contents: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(contents);

    let fixture = txtar.get("dwm/dwm.c").unwrap();
    GitFixture::new(".local/share/ocd/dwm", GitKind::Normal)?
        .stage_and_commit("dwm.c", &fixture.content)?;
    let node = Node::new_open("dwm", &NodeEntry { ..Default::default() })?;
    assert!(node.path().exists());

    let fixture = txtar.get("sh/.shrc").unwrap();
    GitFixture::new("forge/sh.git", GitKind::Bare)?.stage_and_commit(".shrc", &fixture.content)?;
    let node =
        Node::new_open("sh", &NodeEntry { url: "forge/sh.git".into(), ..Default::default() })?;
    assert!(node.path().exists());

    Ok(())
}

#[dir_cases("src/tests/fixture/node_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn smoke_node_deploy(_: &str, contents: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(contents);
    let git = GitFixture::new(".local/share/ocd/node", GitKind::Bare)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let entry = NodeEntry {
        deployment: DeploymentKind::BareAlias(DirAlias::new(&pwd)),
        excluded: Some(vec!["README*".into(), "LICENSE*".into()]),
        ..Default::default()
    };
    let node = Node::new_open("node", &entry)?;

    node.deploy(DeployAction::Deploy)?;
    assert!(node.is_deployed(DeployState::WithoutExcluded)?);

    node.deploy(DeployAction::DeployAll)?;
    assert!(node.is_deployed(DeployState::WithExcluded)?);

    node.deploy(DeployAction::UndeployExcludes)?;
    assert!(node.is_deployed(DeployState::WithoutExcluded)?);

    node.deploy(DeployAction::Undeploy)?;
    assert!(!node.is_deployed(DeployState::WithExcluded)?);

    Ok(())
}
