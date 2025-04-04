// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT


use crate::cluster::*;

use anyhow::{anyhow, Result};
use indoc::{indoc, formatdoc};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use sealed_test::prelude::*;
use std::collections::HashMap;

#[fixture]
fn config() -> String {
    formatdoc! {r#"
        worktree = "$HOME/ocd"
        excludes = ["file1", "file2"]

        # Comment 1
        [node.vim]
        bare_alias = true
        url = ""
        worktree = "$HOME"

        # Comment 2
        [node.sh]
        bare_alias = true
        url = "https://some/url"

        # Comment 3
        [node.bash]
        bare_alias = true
        url = "https://some/url"
        worktree = "$HOME"
        excludes = ["README*", "LICENSE*"]
        depends = ["sh"]

        # Comment 4
        [node.dwm]
        bare_alias = false
        url = "https://some/url"
    "#}
}

#[rstest]
#[sealed_test(env = [("HOME", "/some/path")])]
fn smoke_cluster_from_str_deserialize_with_expanded_worktrees(config: String) -> Result<()> {
    let cluster: Cluster = config.parse()?;
    let expect = Root {
        worktree: Some("/some/path/ocd".into()),
        excludes: Some(vec!["file1".into(), "file2".into()]),
    };
    assert_eq!(cluster.root, expect);

    let mut expect: HashMap<String, Node> = HashMap::new();
    expect.insert(
        "vim".into(),
        Node { bare_alias: true, worktree: Some("/some/path".into()), ..Default::default() },
    );
    expect.insert(
        "sh".into(),
        Node { bare_alias: true, url: "https://some/url".into(), ..Default::default() },
    );
    expect.insert(
        "bash".into(),
        Node {
            bare_alias: true,
            url: "https://some/url".into(),
            worktree: Some("/some/path".into()),
            excludes: Some(vec!["README*".into(), "LICENSE*".into()]),
            depends: Some(vec!["sh".into()]),
        },
    );
    expect.insert(
        "dwm".into(),
        Node { bare_alias: false, url: "https://some/url".into(), ..Default::default() },
    );
    assert_eq!(cluster.nodes, expect);

    Ok(())
}

#[rstest]
#[case::single_node(
    r#"
        [node.foo]
        url = "git@example.org:~user/foo.git"
        bare_alias = true
    "#,
    Ok(())
)]
#[case::acyclic(
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
#[case::depend_self(
    r#"
        [node.foo]
        url = "git@example.org:~user/foo.git"
        bare_alias = true
        depends = ["foo"]
    "#,
    Err(anyhow!("should fail"))
)]
#[case::cycle(
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
    Err(anyhow!("should fail"))
)]
fn smoke_cluster_from_str_acylic_check(
    #[case] nodes: impl AsRef<str>,
    #[case] expect: Result<()>,
) {
    match expect {
        Ok(_) => assert!(nodes.as_ref().parse::<Cluster>().is_ok()),
        Err(_) => assert!(nodes.as_ref().parse::<Cluster>().is_err()),
    }
}

#[rstest]
#[case::node_exists(
    "dwm",
    Ok(Node { url: "https://some/url".into(), ..Default::default() }),
)]
#[case::node_nonexistent("foo", Err(anyhow!("should fail")))]
fn smoke_cluster_get_node(
    config: String,
    #[case] node_name: impl AsRef<str>,
    #[case] expect: Result<Node>,
) -> Result<()> {
    let cluster: Cluster = config.parse()?;
    let result = cluster.get_node(node_name);
    match expect {
        Ok(expect) => assert_eq!(result.unwrap(), &expect),
        Err(_) => assert!(result.is_err()),
    }

    Ok(())
}

#[rstest]
#[case::remove_node(
    "bash",
    Ok(
        Node {
            bare_alias: true,
            url: "https://some/url".into(),
            worktree: Some("/some/path".into()),
            excludes: Some(vec!["README*".into(), "LICENSE*".into()]),
            depends: Some(vec!["sh".into()]),
        }
    ),
    indoc! {r#"
        worktree = "$HOME/ocd"
        excludes = ["file1", "file2"]

        # Comment 1
        [node.vim]
        bare_alias = true
        url = ""
        worktree = "$HOME"

        # Comment 2
        [node.sh]
        bare_alias = true
        url = "https://some/url"

        # Comment 4
        [node.dwm]
        bare_alias = false
        url = "https://some/url"
    "#},
)]
#[case::node_nonexistent(
    "failure",
    Err(anyhow!("should fail")),
    "should fail",
)]
#[sealed_test(env = [("HOME", "/some/path")])]
fn smoke_cluster_remove_node(
    config: String,
    #[case] node_name: impl AsRef<str>,
    #[case] expect_ret: Result<Node>,
    #[case] expect_str: impl AsRef<str>,
) -> Result<()> {
    let mut cluster: Cluster = config.parse()?;
    let result = cluster.remove_node(node_name);

    match expect_ret {
        Ok(expect) => {
            assert_eq!(result.unwrap(), expect);
            assert_eq!(cluster.to_string(), expect_str.as_ref());
        }
        Err(_) => assert!(result.is_err()),
    }

    Ok(())
}

#[rstest]
#[case::new_node(
    config(),
    ("st", Node { bare_alias: false, url: "https://some/url".into(), ..Default::default() }),
    None,
    indoc! {r#"
        worktree = "$HOME/ocd"
        excludes = ["file1", "file2"]

        # Comment 1
        [node.vim]
        bare_alias = true
        url = ""
        worktree = "$HOME"

        # Comment 2
        [node.sh]
        bare_alias = true
        url = "https://some/url"

        # Comment 3
        [node.bash]
        bare_alias = true
        url = "https://some/url"
        worktree = "$HOME"
        excludes = ["README*", "LICENSE*"]
        depends = ["sh"]

        # Comment 4
        [node.dwm]
        bare_alias = false
        url = "https://some/url"

        [node.st]
        bare_alias = false
        url = "https://some/url"
    "#},
)]
#[case::replace_node(
    config(),
    ("vim", Node { bare_alias: false, ..Default::default() }),
    Some(Node { bare_alias: true, worktree: Some("/some/path".into()), ..Default::default() }),
    indoc! {r#"
        worktree = "$HOME/ocd"
        excludes = ["file1", "file2"]

        [node.vim]
        bare_alias = false
        url = ""

        # Comment 2
        [node.sh]
        bare_alias = true
        url = "https://some/url"

        # Comment 3
        [node.bash]
        bare_alias = true
        url = "https://some/url"
        worktree = "$HOME"
        excludes = ["README*", "LICENSE*"]
        depends = ["sh"]

        # Comment 4
        [node.dwm]
        bare_alias = false
        url = "https://some/url"
    "#},
)]
#[case::create_node_table(
    "# Empty\n",
    ("vim", Node { bare_alias: false, ..Default::default() }),
    None,
    indoc!{r#"
        [node.vim]
        bare_alias = false
        url = ""
        # Empty
    "#},
)]
#[sealed_test(env = [("HOME", "/some/path")])]
fn smoke_cluster_add_node(
    #[case] config: impl AsRef<str>,
    #[case] node: (&str, Node),
    #[case] expect_ret: Option<Node>,
    #[case] expect_str: impl AsRef<str>,
) -> Result<()> {
    let mut cluster: Cluster = config.as_ref().parse()?;
    let result = cluster.add_node(node)?;
    assert_eq!(cluster.to_string(), expect_str.as_ref());
    assert_eq!(result, expect_ret);

    Ok(())
}

#[rstest]
#[case::full_path(
    indoc! {r#"
        [node.sh]
        url = "git@example.org:~user/sh.git"
        bare_alias = true
        worktree = "/some/path"

        [node.shell_alias]
        url = "git@example.org:~user/shell_alias.git"
        bare_alias = true
        worktree = "/some/path"

        [node.bash]
        url = "git@example.org:~user/bash.git"
        bare_alias = true
        worktree = "/some/path"
        depends = ["sh", "shell_alias"]
    "#},
    vec![
        (
            "sh",
            Node {
                url: "git@example.org:~user/sh.git".into(),
                bare_alias: true,
                worktree: Some("/some/path".into()),
                ..Default::default()
            },
        ),
        (
            "shell_alias",
            Node {
                url: "git@example.org:~user/shell_alias.git".into(),
                bare_alias: true,
                worktree: Some("/some/path".into()),
                ..Default::default()
            },
        ),
        (
            "bash",
            Node {
                url: "git@example.org:~user/bash.git".into(),
                bare_alias: true,
                worktree: Some("/some/path".into()),
                excludes: None,
                depends: Some(vec!["sh".into(), "shell_alias".into()]),
            },
        ),
    ],
)]
#[case::no_path(
    indoc! {r#"
        [node.dwm]
        url = "git@example.org:~user/dwm.git"
        bare_alias = false
    "#},
    vec![("dwm", Node { url: "git@example.org:~user/dwm.git".into(), ..Default::default() })],
)]
fn smoke_cluster_dependency_iter(
    #[case] config: impl AsRef<str>,
    #[case] mut expect: Vec<(&str, Node)>,
) -> Result<()> {
    let cluster: Cluster = config.as_ref().parse()?;
    let mut result: Vec<(&str, &Node)> = cluster.dependency_iter("bash").collect();

    result.sort_by(|(a, _), (b, _)| a.to_lowercase().cmp(&b.to_lowercase()));
    expect.sort_by(|(a, _), (b, _)| a.to_lowercase().cmp(&b.to_lowercase()));
    for ((name1, node1), (name2, node2)) in expect.iter().zip(result.iter()) {
        assert_eq!(name1, name2);
        assert_eq!(&node1, node2);
    }

    Ok(())
}
