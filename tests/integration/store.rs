// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::{GitFixture, GitKind};

use ocd::{model::*, store::*};

use anyhow::Result;
use run_script::run_script;
use sealed_test::prelude::*;
use simple_test_case::dir_cases;
use simple_txtar::Archive;
use std::fs::write;

#[sealed_test(env = [("XDG_DATA_HOME", ".local/share/ocd")])]
fn root_new_init() -> Result<()> {
    std::env::set_var("HOME", std::env::current_dir()?);
    let entry = RootEntry::try_default()?;
    let root = Root::new_init(&entry)?;
    assert!(root.path().exists());

    Ok(())
}

#[dir_cases("tests/integration/fixtures/root_new_open")]
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
        if file.name == ".config/ocd/root.toml" {
            write(pwd.join(&file.name), file.content.as_bytes())?;
        } else {
            write(pwd.join(".config/ocd").join(&file.name), file.content.as_bytes())?;
        }
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let cluster = Cluster::new()?;
    let root = Root::new_open(&cluster.root)?;
    assert!(root.is_deployed(DeployState::WithoutExcluded)?);

    Ok(())
}

#[dir_cases("tests/integration/fixtures/root_new_clone")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn root_new_clone(_: &str, contents: &str) -> Result<()> {
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

#[dir_cases("tests/integration/fixtures/root_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn root_deploy_action(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/root", GitKind::Bare)?;
    for file in txtar.iter() {
        write(pwd.join(".config/ocd").join(&file.name), file.content.as_bytes())?;
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let cluster = Cluster::new()?;
    let root = Root::new_open(&cluster.root)?;
    root.deploy(DeployAction::Deploy)?;
    assert!(root.is_deployed(DeployState::WithoutExcluded)?);

    Ok(())
}

#[dir_cases("tests/integration/fixtures/root_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn root_undeploy_action(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/root", GitKind::Bare)?;
    for file in txtar.iter() {
        write(pwd.join(".config/ocd").join(&file.name), file.content.as_bytes())?;
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let cluster = Cluster::new()?;
    let root = Root::new_open(&cluster.root)?;
    root.deploy(DeployAction::Undeploy)?;
    assert!(root.is_deployed(DeployState::WithoutExcluded)?);

    Ok(())
}

#[dir_cases("tests/integration/fixtures/root_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn root_deploy_all_action(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/root", GitKind::Bare)?;
    for file in txtar.iter() {
        write(pwd.join(".config/ocd").join(&file.name), file.content.as_bytes())?;
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let cluster = Cluster::new()?;
    let root = Root::new_open(&cluster.root)?;
    root.deploy(DeployAction::DeployAll)?;
    assert!(root.is_deployed(DeployState::WithExcluded)?);

    Ok(())
}

#[dir_cases("tests/integration/fixtures/root_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn root_undeploy_excluded_action(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/root", GitKind::Bare)?;
    for file in txtar.iter() {
        write(pwd.join(".config/ocd").join(&file.name), file.content.as_bytes())?;
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let cluster = Cluster::new()?;
    let root = Root::new_open(&cluster.root)?;
    root.deploy(DeployAction::UndeployExcludes)?;
    assert!(root.is_deployed(DeployState::WithoutExcluded)?);

    Ok(())
}

#[sealed_test(env = [("XDG_DATA_HOME", ".local/share/ocd")])]
fn node_new_init() -> Result<()> {
    std::env::set_var("HOME", std::env::current_dir()?);

    let entry = NodeEntry::builder()?.build();
    let node = Node::new_init("dwm", &entry)?;
    assert!(node.path().exists());
    assert!(!node.is_bare_alias());

    let entry = NodeEntry::builder()?
        .deployment(DeploymentKind::BareAlias, WorkDirAlias::try_default()?)
        .build();
    let node = Node::new_init("vim", &entry)?;
    assert!(node.path().exists());
    assert!(node.is_bare_alias());

    Ok(())
}

#[dir_cases("tests/integration/fixtures/node_new_open")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn node_new_open(_: &str, content: &str) -> Result<()> {
    let txtar = Archive::from(content);
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    // Should just open it since node exists locally!
    let git = GitFixture::new(".local/share/ocd/dwm", GitKind::Normal)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }

    let node = Node::new_open("dwm", &NodeEntry::builder()?.build())?;
    assert!(node.path().exists());

    // Should clone node, because it does not exist locally!
    let git = GitFixture::new("forge/bash.git", GitKind::Normal)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }

    let node = Node::new_open("bash", &NodeEntry::builder()?.url("forge/bash.git").build())?;
    assert!(node.path().exists());

    Ok(())
}

#[dir_cases("tests/integration/fixtures/node_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn node_deploy_action(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/node", GitKind::Bare)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let entry = NodeEntry::builder()?
        .deployment(DeploymentKind::BareAlias, WorkDirAlias::new(&pwd))
        .excluded(["README*", "LICENSE*"])
        .build();
    let node = Node::new_open("node", &entry)?;

    node.deploy(DeployAction::Deploy)?;
    assert!(node.is_deployed(DeployState::WithoutExcluded)?);

    Ok(())
}

#[dir_cases("tests/integration/fixtures/node_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn node_undeploy_action(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/node", GitKind::Bare)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let entry = NodeEntry::builder()?
        .deployment(DeploymentKind::BareAlias, WorkDirAlias::new(&pwd))
        .excluded(["README*", "LICENSE*"])
        .build();
    let node = Node::new_open("node", &entry)?;

    node.deploy(DeployAction::Undeploy)?;
    assert!(!node.is_deployed(DeployState::WithExcluded)?);

    Ok(())
}

#[dir_cases("tests/integration/fixtures/node_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn node_deploy_all_action(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/node", GitKind::Bare)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let entry = NodeEntry::builder()?
        .deployment(DeploymentKind::BareAlias, WorkDirAlias::new(&pwd))
        .excluded(["README*", "LICENSE*"])
        .build();
    let node = Node::new_open("node", &entry)?;

    node.deploy(DeployAction::DeployAll)?;
    assert!(node.is_deployed(DeployState::WithExcluded)?);

    Ok(())
}

#[dir_cases("tests/integration/fixtures/node_deploy")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("XDG_DATA_HOME", ".local/share/ocd"),
])]
fn node_undeploy_excluded_action(_: &str, content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    let git = GitFixture::new(".local/share/ocd/node", GitKind::Bare)?;
    for file in txtar.iter() {
        git.stage_and_commit(&file.name, &file.content)?;
    }
    run_script!(&txtar.comment())?;

    let entry = NodeEntry::builder()?
        .deployment(DeploymentKind::BareAlias, WorkDirAlias::new(&pwd))
        .excluded(["README*", "LICENSE*"])
        .build();
    let node = Node::new_open("node", &entry)?;

    node.deploy(DeployAction::DeployAll)?;
    node.deploy(DeployAction::UndeployExcludes)?;
    assert!(node.is_deployed(DeployState::WithoutExcluded)?);

    Ok(())
}
