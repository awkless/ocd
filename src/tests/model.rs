// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::{model::*, Result};

use indoc::indoc;
use sealed_test::prelude::*;
use simple_test_case::test_case;

#[test_case(
    r#"
        dir_alias = "home_dir"
        excluded = ["rule1", "rule2", "rule3"]
    "#,
    RootEntry {
        dir_alias: DirAlias::new("home/user"),
        excluded: Some(vec!["rule1".into(), "rule2".into(), "rule3".into()]),
    };
    "full fields"
)]
#[test_case(
    "",
    RootEntry {
        dir_alias: DirAlias::new("home/user/.config/ocd"),
        excluded: None,
    };
    "missing all fields"
)]
#[sealed_test(env = [("HOME", "home/user"), ("XDG_CONFIG_HOME", "home/user/.config")])]
fn smoke_cluster_from_str_root_deserialize(config: &str, expect: RootEntry) -> Result<()> {
    let cluster: Cluster = config.parse()?;
    pretty_assertions::assert_eq!(cluster.root, expect);
    Ok(())
}

#[test_case(
    r#"
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"

        [nodes.st]
        deployment = { kind = "normal" }
        url = "https://some/url"
        dependencies = ["prompt"]
        excluded = ["rule1", "rule2", "rule3"]

        [nodes.sh]
        deployment = "bare_alias"
        url = "https://some/url"
        dependencies = ["prompt"]

        [nodes.prompt]
        deployment = { kind = "bare_alias", dir_alias = "some/path" }
        url = "https://some/url"
        excluded = ["rule1", "rule2", "rule3"]
    "#,
    vec![
        (
            "dwm".into(),
            NodeEntry {
                deployment: DeploymentKind::Normal,
                url: "https://some/url".into(),
                ..Default::default()
            }
        ),
        (
            "st".into(),
            NodeEntry {
                deployment: DeploymentKind::Normal,
                url: "https://some/url".into(),
                dependencies: Some(vec!["prompt".into()]),
                excluded: Some(vec!["rule1".into(), "rule2".into(), "rule3".into()]),
            }
        ),
        (
            "sh".into(),
            NodeEntry {
                deployment: DeploymentKind::BareAlias(DirAlias::new("home/user")),
                url: "https://some/url".into(),
                dependencies: Some(vec!["prompt".into()]),
                ..Default::default()
            }
        ),
        (
            "prompt".into(),
            NodeEntry {
                deployment: DeploymentKind::BareAlias(DirAlias::new("some/path")),
                url: "https://some/url".into(),
                excluded: Some(vec!["rule1".into(), "rule2".into(), "rule3".into()]),
                ..Default::default()
            }
        ),
    ];
    "full node set"
)]
#[test_case(
    r#"
        [nodes.dwm]
        url = "https://some/url"

        [nodes.st]
        deployment = "normal"
    "#,
    vec![
        (
            "dwm".into(),
            NodeEntry {
                url: "https://some/url".into(),
                ..Default::default()
            }
        ),
        (
            "st".into(),
            NodeEntry {
                ..Default::default()
            }
        ),
    ];
    "missing fields"
)]
#[sealed_test(env = [("HOME", "home/user")])]
fn smoke_cluster_from_str_node_deserialize(
    config: &str,
    mut expect: Vec<(String, NodeEntry)>,
) -> Result<()> {
    let cluster: Cluster = config.parse()?;
    let mut result = cluster.nodes.into_iter().collect::<Vec<(String, NodeEntry)>>();
    result.sort_by(|(a, _), (b, _)| a.cmp(b));
    expect.sort_by(|(a, _), (b, _)| a.cmp(b));
    pretty_assertions::assert_eq!(result, expect);

    Ok(())
}

#[test_case(
    r#"
        [nodes.foo]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["fail"]

        [nodes.bar]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["snafu"]

        [nodes.baz]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["blah"]
    "#,
    Err(anyhow::anyhow!("should fail"));
    "undefined dependencies"
)]
#[test_case(
    r#"
        [nodes.foo]
        deployment = "normal"
        url = "https://some/url"

        [nodes.bar]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["foo"]

        [nodes.baz]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["bar"]
    "#,
    Ok(());
    "defined dependencies"
)]
#[test]
fn smoke_cluster_from_str_dependency_existence_check(
    config: &str,
    expect: Result<(), anyhow::Error>,
) {
    match expect {
        Ok(_) => config.parse::<Cluster>().is_ok(),
        Err(_) => config.parse::<Cluster>().is_err(),
    };
}

#[test_case(
    r#"
        [nodes.foo]
        deployment = "normal"
        url = "https://some/url"
    "#,
    Ok(());
    "single node"
)]
#[test_case(
    r#"
        [nodes.foo]
        deployment = "normal"
        url = "https://some/url"

        [nodes.bar]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["foo"]

        [nodes.baz]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["bar"]
    "#,
    Ok(());
    "fully acyclic"
)]
#[test_case(
    r#"
        [nodes.foo]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["foo"]
    "#,
    Err(anyhow::anyhow!("should fail"));
    "depend self"
)]
#[test_case(
    r#"
        [nodes.foo]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["bar"]

        [nodes.bar]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["baz"]

        [nodes.baz]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["foo"]
    "#,
    Err(anyhow::anyhow!("should fail"));
    "fully circular"
)]
#[test]
fn smoke_cluster_from_str_acyclic_check(config: &str, expect: Result<(), anyhow::Error>) {
    match expect {
        Ok(_) => assert!(config.parse::<Cluster>().is_ok()),
        Err(_) => assert!(config.parse::<Cluster>().is_err()),
    }
}

#[sealed_test(env = [("CUSTOM_VAR", "some/path")])]
fn smoke_cluster_from_str_expand_dir_aliases() -> Result<()> {
    let config = r#"
        [nodes.vim]
        deployment = { kind = "bare_alias", dir_alias = "$CUSTOM_VAR/vimrc" }

        [nodes.bash]
        deployment = { kind = "bare_alias", dir_alias = "$CUSTOM_VAR/bash" }

        [nodes.fish]
        deployment = { kind = "bare_alias", dir_alias = "$CUSTOM_VAR/fish" }
    "#;
    let cluster: Cluster = config.parse()?;
    let mut expect = vec![
        (
            "vim".to_string(),
            NodeEntry {
                deployment: DeploymentKind::BareAlias(DirAlias::new("some/path/vimrc")),
                ..Default::default()
            },
        ),
        (
            "bash".to_string(),
            NodeEntry {
                deployment: DeploymentKind::BareAlias(DirAlias::new("some/path/bash")),
                ..Default::default()
            },
        ),
        (
            "fish".to_string(),
            NodeEntry {
                deployment: DeploymentKind::BareAlias(DirAlias::new("some/path/fish")),
                ..Default::default()
            },
        ),
    ];
    let mut result = cluster.nodes.into_iter().collect::<Vec<(String, NodeEntry)>>();
    result.sort_by(|(a, _), (b, _)| a.cmp(b));
    expect.sort_by(|(a, _), (b, _)| a.cmp(b));
    pretty_assertions::assert_eq!(result, expect);

    Ok(())
}

#[test_case(
    r#"
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"
    "#,
    "dwm",
    Ok(
        NodeEntry {
            url: "https://some/url".into(),
            ..Default::default()
        }
    );
    "node exists"
)]
#[test_case(
    "# Nothing here",
    "fail",
    Err(anyhow::anyhow!("should fail"));
    "undefined node"
)]
#[test]
fn smoke_cluster_get_node(
    config: &str,
    key: &str,
    expect: Result<NodeEntry, anyhow::Error>,
) -> Result<()> {
    let cluster: Cluster = config.parse()?;
    match expect {
        Ok(expect) => {
            let result = cluster.get_node(key)?;
            pretty_assertions::assert_eq!(&expect, result);
        }
        Err(_) => assert!(cluster.get_node(key).is_err()),
    }

    Ok(())
}

#[test_case(
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"
    "#},
    "st",
    NodeEntry {
        url: "https://some/url".into(),
        ..Default::default()
    },
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"

        [nodes.st]
        deployment = "normal"
        url = "https://some/url"
    "#},
    Ok(None);
    "normal entry"
)]
#[test_case(
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"
    "#},
    "vim",
    NodeEntry {
        deployment: DeploymentKind::BareAlias(DirAlias::new("home/user")),
        url: "https://some/url".into(),
        ..Default::default()
    },
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"

        [nodes.vim]
        deployment = "bare_alias"
        url = "https://some/url"
    "#},
    Ok(None);
    "bare alias home dir"
)]
#[test_case(
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"
    "#},
    "vim",
    NodeEntry {
        deployment: DeploymentKind::BareAlias(DirAlias::new("some/path")),
        url: "https://some/url".into(),
        ..Default::default()
    },
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"

        [nodes.vim]
        deployment = { kind = "bare_alias", dir_alias = "some/path" }
        url = "https://some/url"
    "#},
    Ok(None);
    "bare alias custom dir"
)]
#[test_case(
    indoc! {r#"
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"
    "#},
    "dwm",
    NodeEntry {
        deployment: DeploymentKind::BareAlias(DirAlias::new("some/path")),
        url: "https://some/url".into(),
        ..Default::default()
    },
    indoc! {r#"
        [nodes.dwm]
        deployment = { kind = "bare_alias", dir_alias = "some/path" }
        url = "https://some/url"
    "#},
    Ok(
        Some(
            NodeEntry {
                url: "https://some/url".into(),
                ..Default::default()
            },
        )
    );
    "replace existing node"
)]
#[test_case(
    indoc! {r#"
        # This comment should remain!
    "#},
    "dwm",
    NodeEntry {
        deployment: DeploymentKind::BareAlias(DirAlias::new("some/path")),
        url: "https://some/url".into(),
        ..Default::default()
    },
    indoc! {r#"
        [nodes.dwm]
        deployment = { kind = "bare_alias", dir_alias = "some/path" }
        url = "https://some/url"
        # This comment should remain!
    "#},
    Ok(None);
    "create new nodes table"
)]
#[test_case(
    r#"nodes = "should fail""#,
    "should fail",
    NodeEntry::default(),
    "should fail",
    Err(anyhow::anyhow!("should fail"));
    "not table"
)]
#[sealed_test(env = [("HOME", "home/user")])]
fn smoke_cluster_add_node(
    config: &str,
    key: &str,
    item: NodeEntry,
    str_expect: &str,
    ret_expect: Result<Option<NodeEntry>, anyhow::Error>,
) -> Result<()> {
    let mut cluster: Cluster = config.parse()?;
    match ret_expect {
        Ok(expect) => {
            let result = cluster.add_node(key, item)?;
            pretty_assertions::assert_eq!(result, expect);
            pretty_assertions::assert_eq!(cluster.to_string(), str_expect);
        }
        Err(_) => assert!(cluster.add_node(key, item).is_err()),
    }

    Ok(())
}

#[test_case(
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"

        [nodes.st]
        deployment = "normal"
        url = "https://some/url"
    "#},
    "st",
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"
    "#},
    Ok(
        NodeEntry {
            url: "https://some/url".into(),
            ..Default::default()
        }
    );
    "remove node"
)]
#[test_case(
    r#"nodes = "should fail""#,
    "should fail",
    "should fail",
    Err(anyhow::anyhow!("should fail"));
    "not table"
)]
#[test_case(
    indoc! {r#"
        # This comment should remain!
        [nodes.dwm]
        deployment = "normal"
        url = "https://some/url"
    "#},
    "non-existent",
    "should fail",
    Err(anyhow::anyhow!("should fail"));
    "node entry not found"
)]
#[test]
fn smoke_cluster_remove_node(
    config: &str,
    key: &str,
    str_expect: &str,
    ret_expect: Result<NodeEntry, anyhow::Error>,
) -> Result<()> {
    let mut cluster: Cluster = config.parse()?;
    match ret_expect {
        Ok(expect) => {
            let result = cluster.remove_node(key)?;
            pretty_assertions::assert_eq!(result, expect);
            pretty_assertions::assert_eq!(cluster.to_string(), str_expect);
        }
        Err(_) => assert!(cluster.remove_node(key).is_err()),
    }

    Ok(())
}

#[test_case(
    r#"
        [nodes.sh]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["ps1"]

        [nodes.ps1]
        deployment = "normal"
        url = "https://some/url"

        [nodes.bash]
        deployment = "normal"
        url = "https://some/url"
        dependencies = ["sh"]
    "#,
    vec![
        (
            "sh",
            NodeEntry {
                url: "https://some/url".into(),
                dependencies: Some(vec!["ps1".into()]),
                ..Default::default()
            }
        ),
        (
            "ps1",
            NodeEntry {
                url: "https://some/url".into(),
                ..Default::default()
            }
        ),
        (
            "bash",
            NodeEntry {
                url: "https://some/url".into(),
                dependencies: Some(vec!["sh".into()]),
                ..Default::default()
            }
        ),
    ];
    "full path"
)]
#[test_case(
    r#"
        [nodes.bash]
        deployment = "normal"
        url = "https://some/url"
    "#,
    vec![
        (
            "bash",
            NodeEntry {
                url: "https://some/url".into(),
                ..Default::default()
            }
        ),
    ];
    "no dependencies"
)]
#[test_case(
    r#"
        [nodes.foo]
        deployment = "normal"
        url = "https://some/url"
    "#,
    vec![];
    "no path"
)]
#[test]
fn smoke_cluster_dependency_iter(config: &str, mut expect: Vec<(&str, NodeEntry)>) -> Result<()> {
    let cluster: Cluster = config.parse()?;
    let mut result: Vec<(&str, NodeEntry)> =
        cluster.dependency_iter("bash").map(|(name, node)| (name, node.clone())).collect();

    result.sort_by(|(a, _), (b, _)| a.cmp(b));
    expect.sort_by(|(a, _), (b, _)| a.cmp(b));
    pretty_assertions::assert_eq!(result, expect);
    Ok(())
}
