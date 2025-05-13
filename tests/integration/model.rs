// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use ocd::model::*;

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
        write(&file.name, &file.content.as_bytes())?;
    }

    Ok(())
}

#[track_caller]
fn check_cluster_new(expect: Cluster) -> Result<()> {
    let result = Cluster::new()?;
    pretty_assert_eq!(result, expect);
    Ok(())
}

#[dir_cases("tests/fixtures/cluster_new_valid_setup")]
#[sealed_test(env = [("XDG_CONFIG_HOME", ".config/ocd")])]
fn cluster_new_valid_setup(case: &str, content: &str) -> Result<()> {
    setup_cluster_env(content)?;
    match case {
        "tests/fixtures/cluster_new_valid_setup/root_only.txtar" => {
            let expect = Cluster {
                root: RootEntry::builder()?.deploy_to_home_dir()?.build(),
                nodes: HashMap::default(),
            };
            check_cluster_new(expect)?;
        }
        "tests/fixtures/cluster_new_valid_setup/root_and_nodes.txtar" => {
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

#[dir_cases("tests/fixtures/cluster_new_invalid_setup")]
#[sealed_test(env = [("XDG_CONFIG_HOME", ".config/ocd")])]
fn cluster_new_invalid_setup(_: &str, content: &str) -> Result<()> {
    setup_cluster_env(content)?;
    let result = Cluster::new();
    assert!(result.is_err());
    Ok(())
}
