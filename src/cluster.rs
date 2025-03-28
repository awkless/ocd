// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

//! Cluster configuration management.
//!
//! This module provides basic APIs to manage and manipulate OCD's cluster configuration model.
//! Given that certain OCD commands must modify the contents of the user's cluster configuration,
//! the APIs provided here ensure preservation of existing formatting and whitespace for both
//! deserialization and serialization.
//!
//! ## Clusters
//!
//! The OCD tool operates on a __cluster__. A _cluster_ is a collection of Git repositories that
//! can be deployed together. The cluster is comprised of three repository types: __normal__,
//! __bare-alias__, and __root__. A _normal_ repository is just a regular Git repository whose
//! gitdir and worktree point to the same path. A _bare-alias_ repository is a bare Git repository
//! that uses a target directory as an alias of a worktree. That target directory can be treated
//! like a Git repository without initilization through the OCD tool itself.
//!
//! Finally, a _root_ repository is very special. It represents the root of the cluster itself. It
//! is responsible for containing the cluster configuration file that this module is meant to
//! handle. Thus, all repository deployment for a given cluster definition originates right here in
//! the root repository. However, a cluster can only have _one_ root, i.e., one repository
//! containing one copy of the cluster configuration file to deploy from.
//!
//! The concept of a cluster provides the user with a lot of flexibility in how they choose to
//! organize their dotfile configurations. The user can store dotfiles in separate repositories and
//! plug them into a given cluster whenever they want. The user can also maintain a monolithic
//! repository containing every possible configuration file they use. Whatever method of
//! organization the user chooses, the OCD tool's cluster configuration model will provide flexible
//! and adaptable support.

use anyhow::{anyhow, Context, Result};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
};
use toml_edit::{Array, DocumentMut, Item, Key, Table, Value};

/// Format preserving cluster configuration parser.
///
/// Obtains valid parsing of user's cluster configuration definition in deserialzed form. Provides
/// additional utilities to make it easier to extract and serialize cluster data for further
/// manipulation when needed. This type only operates on strings. Caller is responsible for file
/// I/O.
///
/// ## Cluster definition
///
/// A __cluster definition__ is comprised of two basic components: __root__ and __node__. A root
/// configures the root repository, and a __node__ configures a normal or bare-alias repository for
/// deployment. The root uses the root table, while a node uses a special reserved table named
/// "node" with a dotted key representing the name of the node entry itself, e.g., "node.vim".
#[derive(Default, Debug)]
pub struct Cluster {
    /// Root of cluster definition.
    pub root: Root,

    /// All node entries in cluster definition.
    pub nodes: HashMap<String, Node>,

    document: DocumentMut,
}

impl Cluster {
    /// Construct new empty cluster definition.
    pub fn new() -> Self {
        Cluster::default()
    }

    /// Get single node by name.
    ///
    /// ## Errors
    ///
    /// Will fail if node does not exist in cluster.
    pub fn get_node(&self, name: impl AsRef<str>) -> Result<&Node> {
        self.nodes
            .get(name.as_ref())
            .ok_or(anyhow!("Node '{}' not defined in cluster", name.as_ref()))
    }

    /// Iterate through all dependencies of a target node by name.
    ///
    /// Provides full path through each dependency through a given node, include the node itself.
    pub fn dependency_iter(&self, node: impl Into<String>) -> DependencyIter<'_> {
        let mut stack = VecDeque::new();
        stack.push_front(node.into());
        DependencyIter { graph: &self.nodes, visited: HashSet::new(), stack }
    }

    /// Add new node into cluster.
    ///
    /// Will insert new node into cluster, returning [`None`] if the node was actually new, or
    /// [`Some`] containing the old node it replaced if it was not new.
    ///
    /// ## Errors
    ///
    /// Will fail "node" table was not actually defined as a table.
    pub fn add_node(&mut self, node: (impl AsRef<str>, Node)) -> Result<Option<Node>> {
        let (name, node) = node;

        let (key, item) = node.to_toml(name.as_ref());
        let table = if let Some(item) = self.document.get_mut("node") {
            item.as_table_mut().ok_or(anyhow!("Node table not defined as a table"))?
        } else {
            let mut new_table = Table::new();
            new_table.set_implicit(true);
            self.document.insert("node", Item::Table(new_table));
            self.document["node"].as_table_mut().unwrap()
        };
        table.insert(key.get(), item);

        Ok(self.nodes.insert(name.as_ref().into(), node))
    }

    /// Remove existing node from cluster.
    ///
    /// ## Errors
    ///
    /// Will fail if target node does not exist in cluster, or "node" table was not defined as a
    /// table.
    pub fn remove_node(&mut self, node: impl AsRef<str>) -> Result<Node> {
        self.document
            .get_mut("node")
            .and_then(|n| n.as_table_mut())
            .ok_or(anyhow!("Node table not defined"))?
            .remove(node.as_ref())
            .ok_or(anyhow!("Node '{}' not defined in cluster", node.as_ref()))?;

        self.nodes
            .remove(node.as_ref())
            .ok_or(anyhow!("Node '{}' not defined in hashmap", node.as_ref()))
    }

    fn acyclic_check(&self) -> Result<()> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut count: usize = 0;
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        for (name, node) in &self.nodes {
            in_degree.entry(name.clone()).or_insert(0);
            for depend in node.depends.iter().flatten() {
                *in_degree.entry(depend.clone()).or_insert(0) += 1;
            }
        }

        for (name, degree) in &in_degree {
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

    fn expand_worktrees(&mut self) -> Result<()> {
        if let Some(worktree) = &self.root.worktree {
            self.root.worktree = Some(
                shellexpand::full(worktree.to_string_lossy().as_ref())
                    .with_context(|| "Failed to expand root worktree")?
                    .into_owned()
                    .into(),
            );
        }

        for (name, node) in &mut self.nodes {
            if let Some(worktree) = &node.worktree {
                node.worktree = Some(
                    shellexpand::full(worktree.to_string_lossy().as_ref())
                        .with_context(|| format!("Failed to expand {name} worktree"))?
                        .into_owned()
                        .into(),
                );
            }
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

        let mut cluster = Self { document, root, nodes };
        cluster.acyclic_check()?;
        cluster.expand_worktrees()?;

        Ok(cluster)
    }
}

impl std::fmt::Display for Cluster {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.document)
    }
}

/// Iterator for generating valid node dependency path.
///
/// ## Invariants
///
/// 1. Nodes and their dependencies are acyclic.
#[derive(Debug)]
pub struct DependencyIter<'cluster> {
    graph: &'cluster HashMap<String, Node>,
    visited: HashSet<String>,
    stack: VecDeque<String>,
}

impl<'cluster> Iterator for DependencyIter<'cluster> {
    type Item = (&'cluster str, &'cluster Node);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = self.stack.pop_front() {
            let Some((name, node)) = self.graph.get_key_value(&node) else {
                log::error!("Node '{node}' not defined in cluster");
                return None;
            };

            for depend in node.depends.iter().flatten() {
                if !self.visited.contains(depend) {
                    self.stack.push_front(depend.clone());
                    self.visited.insert(depend.clone());
                }
            }

            return Some((name.as_ref(), node));
        }

        None
    }
}

/// Configuration options for root of cluster.
#[derive(Default, Debug, Eq, PartialEq, Clone)]
pub struct Root {
    /// Target directory to act as the worktree alias for deployment.
    pub worktree: Option<PathBuf>,

    /// List of files to exclude from deployment using sparse checkout.
    pub excludes: Option<Vec<String>>,
}

impl Root {
    /// Construct new empty root.
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

/// Configuration options for node entry in cluster.
#[derive(Default, Debug, Eq, PartialEq, Clone)]
pub struct Node {
    /// True if node is bare-alias, false if normal.
    pub bare_alias: bool,

    /// URL to clone node repository from.
    pub url: String,

    /// Target directory to act as the worktree alias for deployment.
    pub worktree: Option<PathBuf>,

    /// List of files to exclude from deployment using sparse checkout.
    pub excludes: Option<Vec<String>>,

    /// List of node dependencies to include for deployment.
    pub depends: Option<Vec<String>>,
}

impl Node {
    /// Construct new empty node.
    pub fn new() -> Self {
        Node::default()
    }

    /// Convert [`Node`] to valid TOML entry.
    ///
    /// Will ensure that optional fields are left out of the generated TOML data when defined as
    /// [`None`] for cleaner serialized output.
    pub fn to_toml(&self, name: &str) -> (Key, Item) {
        let mut node = Table::new();
        node.insert("bare_alias", Item::Value(Value::from(self.bare_alias)));
        node.insert("url", Item::Value(Value::from(&self.url)));

        if let Some(worktree) = &self.worktree {
            node.insert(
                "worktree",
                Item::Value(Value::from(worktree.to_string_lossy().into_owned())),
            );
        }

        if let Some(excludes) = &self.excludes {
            node.insert("excludes", Item::Value(Value::Array(Array::from_iter(excludes))));
        }

        if let Some(depends) = &self.depends {
            node.insert("excludes", Item::Value(Value::Array(Array::from_iter(depends))));
        }

        let key = Key::new(name);
        let value = Item::Table(node);
        (key, value)
    }
}

impl<'toml> From<&'toml Item> for Node {
    fn from(item: &'toml Item) -> Self {
        Self {
            bare_alias: item.get("bare_alias").and_then(Item::as_bool).unwrap_or_default(),
            url: item.get("url").and_then(|n| n.as_str().map(Into::into)).unwrap_or_default(),
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

    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use sealed_test::prelude::*;

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
            Node { bare_alias: true, url: "https://some/url".into(), ..Default::default() },
        );
        expect.insert(
            "bash".into(),
            Node {
                bare_alias: true,
                url: "https://some/url".into(),
                worktree: Some("home".into()),
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

    #[test]
    fn smoke_cluster_from_str_acylic_check() {
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
    }

    #[sealed_test(env = [("HOME", "/some/path")])]
    fn smoke_cluster_from_str_expand_worktrees() -> Result<()> {
        let config = r#"
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
        "#;
        let cluster: Cluster = config.parse()?;

        assert_eq!(cluster.root.worktree, Some(PathBuf::from("/some/path/ocd")));
        for node in cluster.nodes.values() {
            if let Some(worktree) = &node.worktree {
                assert_eq!(worktree, &PathBuf::from("/some/path"));
            }
        }

        Ok(())
    }

    #[test]
    fn smoke_cluster_remove_node() -> Result<()> {
        let config = r#"
            # Comment should be here.
            [node.sh]
            url = "git@example.org:~user/sh.git"
            bare_alias = true

            [node.bash]
            url = "git@example.org:~user/bash.git"
            bare_alias = true
            depends = ["sh"]

            [node.dwm]
            url = "git@example.org:~user/dwm.git"
            bare_alias = false
        "#;
        let mut cluster: Cluster = config.parse()?;
        cluster.remove_node("bash")?;
        let expect = r#"
            # Comment should be here.
            [node.sh]
            url = "git@example.org:~user/sh.git"
            bare_alias = true

            [node.dwm]
            url = "git@example.org:~user/dwm.git"
            bare_alias = false
        "#;
        assert_eq!(cluster.to_string(), expect);
        assert!(cluster.remove_node("nonexistent").is_err());

        Ok(())
    }

    #[test]
    fn smoke_cluster_add_node() -> Result<()> {
        let mut cluster = Cluster::new();
        let node = Node { bare_alias: false, ..Default::default() };
        let expect = indoc! {r#"
            [node.sh]
            bare_alias = false
            url = ""
        "#};
        cluster.add_node(("sh", node))?;
        assert_eq!(cluster.to_string(), expect);

        let node_exists = indoc! {r#"
            # Comment should be here.
            [node.sh]
            url = "git@example.org:~user/sh.git"
            bare_alias = true
        "#};
        let mut cluster: Cluster = node_exists.parse()?;
        let node = Node { url: "git@example.org:~user/dwm.git".into(), ..Default::default() };
        cluster.add_node(("dwm", node))?;
        let expect = indoc! {r#"
            # Comment should be here.
            [node.sh]
            url = "git@example.org:~user/sh.git"
            bare_alias = true

            [node.dwm]
            bare_alias = false
            url = "git@example.org:~user/dwm.git"
        "#};
        assert_eq!(cluster.to_string(), expect);

        Ok(())
    }

    #[test]
    fn smoke_cluster_get_node() -> Result<()> {
        let config = r#"
            [node.sh]
            url = "git@example.org:~user/sh.git"
            bare_alias = true
            worktree = "/some/path"
        "#;
        let cluster: Cluster = config.parse()?;

        let result = cluster.get_node("sh")?;
        let expect = Node {
            url: "git@example.org:~user/sh.git".into(),
            bare_alias: true,
            worktree: Some("/some/path".into()),
            ..Default::default()
        };
        assert_eq!(result, &expect);

        let result = cluster.get_node("nonexistent");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn smoke_cluster_dependency_iter() -> Result<()> {
        let config = r#"
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

            [node.dwm]
            url = "git@example.org:~user/dwm.git"
            bare_alias = false
        "#;
        let cluster: Cluster = config.parse()?;

        let mut result: Vec<(&str, &Node)> = cluster.dependency_iter("bash").collect();
        let mut expect = vec![
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
        ];
        result.sort_by(|(a, _), (b, _)| a.to_lowercase().cmp(&b.to_lowercase()));
        expect.sort_by(|(a, _), (b, _)| a.to_lowercase().cmp(&b.to_lowercase()));
        for ((name1, node1), (name2, node2)) in expect.iter().zip(result.iter()) {
            assert_eq!(name1, name2);
            assert_eq!(&node1, node2);
        }

        let result: Vec<(&str, &Node)> = cluster.dependency_iter("dwm").collect();
        let expect =
            [("dwm", Node { url: "git@example.org:~user/dwm.git".into(), ..Default::default() })];
        for ((name1, node1), (name2, node2)) in expect.iter().zip(result.iter()) {
            assert_eq!(name1, name2);
            assert_eq!(&node1, node2);
        }

        Ok(())
    }
}
