// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use ocd::model::{
    cluster::{Cluster, DeploymentKind, NodeEntry, RootEntry, WorkDirAlias},
    home_dir,
};

use anyhow::Result;
use pretty_assertions::assert_eq as pretty_assert_eq;
use sealed_test::prelude::*;
use simple_test_case::dir_cases;
use simple_txtar::Archive;
use std::{collections::HashMap, fs::write};

fn setup_cluster_env(content: &str) -> Result<()> {
    let pwd = std::env::current_dir()?;
    std::fs::create_dir_all(".config/ocd/nodes")?;
    std::env::set_var("HOME", &pwd);

    let txtar = Archive::from(content);
    for file in txtar.iter() {
        write(&file.name, file.content.as_bytes())?;
    }

    Ok(())
}

#[track_caller]
fn check_cluster_new(expect: Cluster) -> Result<()> {
    let result = Cluster::new()?;
    pretty_assert_eq!(result, expect);
    Ok(())
}

#[dir_cases("tests/integration/fixtures/cluster_new_valid_setup")]
#[sealed_test(env = [("XDG_CONFIG_HOME", ".config/ocd")])]
fn cluster_new_valid_setup(case: &str, content: &str) -> Result<()> {
    setup_cluster_env(content)?;
    match case {
        "tests/integration/fixtures/cluster_new_valid_setup/root_only.txtar" => {
            let expect = Cluster {
                root: RootEntry::builder()?.deploy_to_home_dir()?.build(),
                nodes: HashMap::default(),
            };
            check_cluster_new(expect)?;
        }
        "tests/integration/fixtures/cluster_new_valid_setup/root_and_nodes.txtar" => {
            let mut nodes = HashMap::new();
            nodes.insert(
                "vim".into(),
                NodeEntry::builder()?
                    .deployment(DeploymentKind::BareAlias, WorkDirAlias::new(home_dir()?))
                    .url("https://some/url")
                    .build(),
            );
            nodes.insert("dwm".into(), NodeEntry::builder()?.url("https://some/url").build());
            let expect =
                Cluster { root: RootEntry::builder()?.deploy_to_config_dir()?.build(), nodes };
            check_cluster_new(expect)?;
        }
        &_ => unreachable!("No code for this yet!"),
    }

    Ok(())
}

#[dir_cases("tests/integration/fixtures/cluster_new_invalid_setup")]
#[sealed_test(env = [("XDG_CONFIG_HOME", ".config/ocd")])]
fn cluster_new_invalid_setup(_: &str, content: &str) -> Result<()> {
    setup_cluster_env(content)?;
    let result = Cluster::new();
    assert!(result.is_err());
    Ok(())
}

#[dir_cases("tests/integration/fixtures/cluster_new_acyclic_check")]
#[sealed_test(env = [("XDG_CONFIG_HOME", ".config/ocd")])]
fn cluster_new_acyclic_check(case: &str, content: &str) -> Result<()> {
    setup_cluster_env(content)?;
    match case {
        "tests/integration/fixtures/cluster_new_acyclic_check/no_dependencies.txtar"
        | "tests/integration/fixtures/cluster_new_acyclic_check/full_dependencies.txtar" => {
            let result = Cluster::new();
            assert!(result.is_ok());
        }
        "tests/integration/fixtures/cluster_new_acyclic_check/depend_self.txtar"
        | "tests/integration/fixtures/cluster_new_acyclic_check/full_cycle.txtar" => {
            let result = Cluster::new();
            assert!(result.is_err());
        }
        &_ => unreachable!("No code for this yet!"),
    }
    Ok(())
}

#[dir_cases("tests/integration/fixtures/cluster_new_dependency_existence_check")]
#[sealed_test(env = [("XDG_CONFIG_HOME", ".config/ocd")])]
fn cluster_new_dependency_existence_check(case: &str, content: &str) -> Result<()> {
    setup_cluster_env(content)?;
    match case {
        "tests/integration/fixtures/cluster_new_dependency_existence_check/defined_dependencies.txtar" => {
            let result = Cluster::new();
            assert!(result.is_ok());
        }
        "tests/integration/fixtures/cluster_new_dependency_existence_check/undefined_dependencies.txtar" => {
            let result = Cluster::new();
            assert!(result.is_err());
        }
        &_ => unreachable!("No code for this yet!"),
    }
    Ok(())
}

#[dir_cases("tests/integration/fixtures/cluster_new_expand_work_dir_aliases")]
#[sealed_test(env = [
    ("XDG_CONFIG_HOME", ".config/ocd"),
    ("EXPAND_ME1", "some/path"),
    ("EXPAND_ME2", "some/path"),
    ("EXPAND_ME3", "some/path"),
])]
fn cluster_new_expand_work_dir_aliases(_: &str, content: &str) -> Result<()> {
    setup_cluster_env(content)?;
    let cluster = Cluster::new()?;
    for node in cluster.nodes.values() {
        pretty_assert_eq!(node.settings.deployment.work_dir_alias, WorkDirAlias::new("some/path"));
    }
    Ok(())
}

#[track_caller]
fn check_cluster_dependency_iter(target: &str, mut expect: Vec<(String, NodeEntry)>) -> Result<()> {
    let cluster = Cluster::new()?;
    let mut result: Vec<(String, NodeEntry)> = cluster
        .dependency_iter(target)
        .map(|(name, node)| (name.to_string(), node.clone()))
        .collect();
    result.sort_by(|(a, _), (b, _)| a.cmp(b));
    expect.sort_by(|(a, _), (b, _)| a.cmp(b));
    pretty_assert_eq!(result, expect);
    Ok(())
}

#[dir_cases("tests/integration/fixtures/cluster_dependency_iter")]
#[sealed_test(env = [("XDG_CONFIG_HOME", ".config/ocd")])]
fn cluster_dependency_iter(case: &str, content: &str) -> Result<()> {
    setup_cluster_env(content)?;
    match case {
        "tests/integration/fixtures/cluster_dependency_iter/full_dependency_path.txtar" => {
            let expect = vec![
                ("node_00".into(), NodeEntry::builder()?.url("https://some/url").build()),
                ("node_01".into(), NodeEntry::builder()?.url("https://some/url").build()),
                ("node_02".into(), NodeEntry::builder()?.url("https://some/url").build()),
                (
                    "node_03".into(),
                    NodeEntry::builder()?
                        .url("https://some/url")
                        .dependencies(["node_00", "node_01", "node_02"])
                        .build(),
                ),
            ];
            check_cluster_dependency_iter("node_03", expect)?;
        }
        "tests/integration/fixtures/cluster_dependency_iter/no_dependencies.txtar" => {
            let expect =
                vec![("node_00".into(), NodeEntry::builder()?.url("https://some/url").build())];
            check_cluster_dependency_iter("node_00", expect)?;
        }
        &_ => unreachable!("No code for this case yet!"),
    }
    Ok(())
}
