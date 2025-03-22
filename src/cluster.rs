// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

use anyhow::{anyhow, Context, Result};
use std::{collections::{VecDeque, HashSet, HashMap}, path::PathBuf};
use toml_edit::{DocumentMut, Item, Table};

#[derive(Default, Debug)]
pub struct Cluster {
    document: DocumentMut,
    root: Root,
    nodes: HashMap<String, Node>,
}

impl Cluster {
    pub fn new() -> Self {
        Cluster::default()
    }

    pub fn get_root(&self) -> &Root {
        &self.root
    }

    fn acyclic_check(&self) -> Result<()> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut count: usize = 0;
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        for (name, node) in self.nodes.iter() {
            in_degree.entry(name.clone()).or_insert(0);
            for depend in node.depends.iter().flatten() {
                *in_degree.entry(depend.clone()).or_insert(0) += 1;
            }
        }

        for (name, degree) in in_degree.iter() {
            if *degree == 0 {
                queue.push_back(name.clone());
            }
        }

        while let Some(current) = queue.pop_front() {
            count += 1;
            for depend in self.nodes[&current].depends.iter().flatten() {
                *in_degree.get_mut(depend).unwrap() -= 1;
                if *in_degree.get(depend).unwrap() == 0 {
                    queue.push_back(depend.clone());
                }
            }
            visited.insert(current);
        }

        if count != self.nodes.len() {
            let cycle: Vec<String> = self
                .nodes
                .iter()
                .filter(|(name, _)| !visited.contains(*name))
                .map(|(name, _)| name.clone())
                .collect();
            return Err(anyhow!("Cluster contains cycle(s): {cycle:?}"));
        }

        Ok(())
    }
}

impl std::str::FromStr for Cluster {
    type Err = anyhow::Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let document: DocumentMut = data.parse().with_context(|| "Bad parse")?;
        let root = Root::from(document.as_table());
        let nodes = if let Some(node_table) = document.get("node").and_then(|n| n.as_table()) {
            node_table
                .iter()
                .map(|(key, value)| (key.into(), Node::from(value)))
                .collect::<HashMap<String, Node>>()
        } else {
            HashMap::new()
        };

        let cluster = Self { document, root, nodes };
        cluster.acyclic_check()?;

        Ok(cluster)
    }
}

impl std::fmt::Display for Cluster {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.document)
    }
}

#[derive(Default, Debug, Eq, PartialEq, Clone)]
pub struct Root {
    pub worktree: Option<PathBuf>,
    pub excludes: Option<Vec<String>>,
}

impl Root {
    pub fn new() -> Self {
        Root::default()
    }
}

impl<'toml> From<&'toml Table> for Root {
    fn from(table: &'toml Table) -> Self {
        let mut root = Root { ..Default::default() };
        root.worktree = table.get("worktree").and_then(|n| n.as_str().map(Into::into));
        root.excludes = table.get("excludes").and_then(|n| {
            n.as_array()
                .map(|a| a.into_iter().map(|s| s.as_str().unwrap_or_default().into()).collect())
        });
        root
    }
}

#[derive(Default, Debug, Eq, PartialEq, Clone)]
pub struct Node {
    pub bare_alias: bool,
    pub url: Option<String>,
    pub worktree: Option<PathBuf>,
    pub excludes: Option<Vec<String>>,
    pub depends: Option<Vec<String>>,
}

impl Node {
    pub fn new() -> Self {
        Node::default()
    }
}

impl<'toml> From<&'toml Item> for Node {
    fn from(item: &'toml Item) -> Self {
        Self {
            bare_alias: item.get("bare_alias").and_then(Item::as_bool).unwrap_or_default(),
            url: item.get("url").and_then(|n| n.as_str().map(Into::into)),
            worktree: item.get("worktree").and_then(|n| n.as_str().map(Into::into)),
            excludes: item.get("excludes").and_then(|n| {
                n.as_array()
                    .map(|a| a.into_iter().map(|s| s.as_str().unwrap_or_default().into()).collect())
            }),
            depends: item.get("depends").and_then(|n| {
                n.as_array()
                    .map(|a| a.into_iter().map(|s| s.as_str().unwrap_or_default().into()).collect())
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_cluster_from_str_extract_root_and_nodes() -> Result<()> {
        let toml = r#"
            worktree = "/some/path"
            excludes = ["file1", "file2"]

            [node.vim]
            bare_alias = true
            worktree = ".vim"

            [node.sh]
            bare_alias = true
            url = "https://some/url"

            [node.bash]
            bare_alias = true
            url = "https://some/url"
            worktree = "home"
            excludes = ["README*", "LICENSE*"]
            depends = ["sh"]

            [node.dwm]
            bare_alias = false
            url = "https://some/url"
        "#;
        let cluster: Cluster = toml.parse()?;

        let expect = Root {
            worktree: Some("/some/path".into()),
            excludes: Some(vec!["file1".into(), "file2".into()]),
        };
        assert_eq!(cluster.root, expect);

        let mut expect: HashMap<String, Node> = HashMap::new();
        expect.insert(
            "vim".into(),
            Node { bare_alias: true, worktree: Some(".vim".into()), ..Default::default() },
        );
        expect.insert(
            "sh".into(),
            Node { bare_alias: true, url: Some("https://some/url".into()), ..Default::default() },
        );
        expect.insert(
            "bash".into(),
            Node {
                bare_alias: true,
                url: Some("https://some/url".into()),
                worktree: Some("home".into()),
                excludes: Some(vec!["README*".into(), "LICENSE*".into()]),
                depends: Some(vec!["sh".into()]),
            },
        );
        expect.insert(
            "dwm".into(),
            Node { bare_alias: false, url: Some("https://some/url".into()), ..Default::default() },
        );
        assert_eq!(cluster.nodes, expect);

        Ok(())
    }

    #[test]
    fn smoke_cluster_from_str_acylic_check() -> Result<()> {
        let single_node = r#"
            [node.foo]
            url = "git@example.org:~user/foo.git"
            bare_alias = true
        "#;
        assert!(single_node.parse::<Cluster>().is_ok());

        let acyclic = r#"
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
        "#;
        assert!(acyclic.parse::<Cluster>().is_ok());

        let depend_self = r#"
            [node.foo]
            url = "git@example.org:~user/foo.git"
            bare_alias = true
            depends = ["foo"]
        "#;
        assert!(depend_self.parse::<Cluster>().is_err());

        let cycle = r#"
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
        "#;
        assert!(cycle.parse::<Cluster>().is_err());

        Ok(())
    }
}
