// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::config::cluster::*;

use anyhow::{anyhow, Result};
use rstest::{fixture, rstest};
use sealed_test::prelude::*;
use std::{collections::HashSet, path::PathBuf};

#[fixture]
fn config() -> String {
    r#"
        worktree = "$HOME/ocd"

        [node.sh]
        url = "git@example.org:~user/sh.git"
        bare_alias = true
        worktree = "$HOME"

        [node.shell_alias]
        url = "git@example.org:~user/shell_alias.git"
        bare_alias = true
        worktree = "$HOME"

        [node.bash]
        url = "git@example.org:~user/bash.git"
        bare_alias = true
        worktree = "$HOME"
        depends = ["sh", "shell_alias"]

        [node.dwm]
        url = "git@example.org:~user/dwm.git"
        bare_alias = false
    "#
    .to_string()
}

#[rstest]
#[case::no_cycle(
    r#"
        [node.foo]
        url = "git@example.org:~user/foo.git"
        bare_alias = true

        [node.bar]
        url = "git@example.org:~user/bar.git"
        bare_alias = true
        depends = ["foo"]

        [node.baz]
        url = "git@example.org:~user/baz.git"
        bare_alias = true
        depends = ["bar"]
    "#,
    Ok(())
)]
#[case::no_cycle(
    r#"
        [node.foo]
        url = "git@example.org:~user/foo.git"
        bare_alias = true
    "#,
    Ok(())
)]
#[case::no_cycle(
    r#"
        [node.foo]
        url = "git@example.org:~user/foo.git"
        bare_alias = true
        depends = ["bar", "baz"]

        [node.bar]
        url = "git@example.org:~user/bar.git"
        bare_alias = true

        [node.baz]
        url = "git@example.org:~user/baz.git"
        bare_alias = true
    "#,
    Ok(())
)]
#[case::catch_cycle(
    r#"
        [node.foo]
        url = "git@example.org:~user/foo.git"
        bare_alias = true
        depends = ["baz"]

        [node.bar]
        url = "git@example.org:~user/bar.git"
        bare_alias = true
        depends = ["foo"]

        [node.baz]
        url = "git@example.org:~user/baz.git"
        bare_alias = true
        depends = ["bar"]
    "#,
    Err(anyhow!("fail")),
)]
#[case::catch_cycle(
    r#"
        [node.foo]
        url = "git@example.org:~user/foo.git"
        bare_alias = true
        depends = ["foo"]
    "#,
    Err(anyhow!("fail")),
)]
#[case::catch_cycle(
    r#"
        [node.foo]
        url = "git@example.org:~user/foo.git"
        bare_alias = true

        [node.bar]
        url = "git@example.org:~user/bar.git"
        bare_alias = true
        depends = ["foo", "bar", "baz"]

        [node.baz]
        url = "git@example.org:~user/baz.git"
        bare_alias = true
    "#,
    Err(anyhow!("fail")),
)]
fn smoke_cluster_cycle_check(#[case] input: &str, #[case] expect: Result<()>) -> Result<()> {
    let result: Result<Cluster> = input.parse();
    match expect {
        Ok(()) => assert!(result.is_ok()),
        Err(_) => assert!(result.is_err()),
    }
    Ok(())
}

#[rstest]
#[sealed_test(env = [("HOME", "/some/path")])]
fn smoke_cluster_expand_worktrees(config: String) -> Result<()> {
    let cluster: Cluster = config.parse()?;
    assert_eq!(cluster.worktree, Some(PathBuf::from("/some/path/ocd")));
    for (_, node) in cluster.node.iter() {
        if let Some(worktree) = &node.worktree {
            assert_eq!(worktree, &PathBuf::from("/some/path"));
        }
    }

    Ok(())
}

#[rstest]
#[case::deps(
    "bash",
    vec![
        Node {
            url: "git@example.org:~user/sh.git".into(),
            bare_alias: true,
            worktree: Some("/some/path".into()),
            ..Default::default()
        },
        Node {
            url: "git@example.org:~user/shell_alias.git".into(),
            bare_alias: true,
            worktree: Some("/some/path".into()),
            ..Default::default()
        },
        Node {
            url: "git@example.org:~user/bash.git".into(),
            bare_alias: true,
            worktree: Some("/some/path".into()),
            excludes: None,
            depends: Some(vec!["sh".into(), "shell_alias".into()]),
        },
    ],
)]
#[case::no_deps(
    "dwm",
    vec![
        Node {
            url: "git@example.org:~user/dwm.git".into(),
            ..Default::default()
        }
    ],
)]
#[sealed_test(env = [("HOME", "/some/path")])]
fn smoke_cluster_dependency_iter(
    config: String,
    #[case] node: &str,
    #[case] expect: Vec<Node>,
) -> Result<()> {
    let cluster: Cluster = config.parse()?;
    let result: HashSet<&Node> = cluster.dependency_iter(node).collect();
    assert!(expect.iter().all(|node| result.contains(&node)));
    Ok(())
}
