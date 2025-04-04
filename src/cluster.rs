// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

//! Cluster configuration management.
//!
//! This module provides utilities to manage and manipulate OCD's cluster configuration model.
//! Given that certain OCD commands must modify the contents of the user's cluster configuration,
//! the APIs provided here ensure preservation of existing formatting and whitespace for both
//! deserialization and serialization.
//!
//! # Cluster configuration model
//!
//! OCD's cluster configuration model is comprised of two basic components: __root__ and __node__.
//! A _root_ configures the root repository, and a _node_ configures a normal or bare-alias
//! repository for deployment.
//!
//! Currently, the OCD tool uses the TOML data exchange format for writing and storing user-defined
//! cluster definitions. The root uses the root table, while a node uses a special reserved table
//! named "node" with a dotted key representing the name of the node entry itself,
//! e.g., "[node.vim]".
//!
//! The root of a cluster definition mainly determines the worktree alias path, and the set of
//! files to exclude from its index upon deployment to said worktree alias path. Both of these
//! configuration settings are optional.
//!
//! A node of a cluster definition contains the following settings:
//!   - URL to clone node repository from.
//!   - Boolean flag to determine if it is bare-alias or not.
//!   - Target worktree alias path to deploy to.
//!   - List of files to exclude from index upon deployment.
//!   - List of other nodes in cluster to deploy as dependencies.
//!
//! The URL and boolean flag are mandatory, while everything else is optional.
//!
//! See [`vcs`](crate::vcs) module for more information about how repositories for a cluster
//! definition are managed through version control.

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
/// # Invariants
///
/// - Node dependencies are acyclic.
/// - Worktree paths are always expanded.
#[derive(Default, Debug)]
pub struct Cluster {
    /// Root of cluster definition.
    pub root: Root,

    /// All node entries in cluster definition represented as DAG.
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
    /// # Errors
    ///
    /// Will fail if node does not exist in cluster.
    pub fn get_node(&self, name: impl AsRef<str>) -> Result<&Node> {
        self.nodes
            .get(name.as_ref())
            .ok_or(anyhow!("Node '{}' not defined in cluster", name.as_ref()))
    }

    /// Iterate through all dependencies of a target node by name.
    ///
    /// Provides full path through each dependency of target node inclusively.
    pub fn dependency_iter(&self, node: impl Into<String>) -> DependencyIter<'_> {
        let mut stack = VecDeque::new();
        stack.push_front(node.into());
        DependencyIter { graph: &self.nodes, visited: HashSet::new(), stack }
    }

    /// Add node into cluster.
    ///
    /// Will insert new node into cluster, returning [`None`] if the node was actually new, or
    /// [`Some`] containing the old node it replaced if it was not new. The "node" table will be
    /// constructed if it does not already exist.
    ///
    /// # Errors
    ///
    /// Will fail if "node" item exists, but was not actually defined as a table.
    pub fn add_node(&mut self, node: (impl AsRef<str>, Node)) -> Result<Option<Node>> {
        let (name, node) = node;

        let (key, item) = node.to_toml(name.as_ref());
        let table = if let Some(item) = self.document.get_mut("node") {
            item.as_table_mut().ok_or(anyhow!("Node table not defined as a table"))?
        } else {
            // INVARIANT: Construct new "node" table to insert node entry into.
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
    /// # Errors
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
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        // INVARIANT: The in-degree of each node is the sum of all incoming edges to each
        // destination node.
        for (name, node) in &self.nodes {
            in_degree.entry(name.clone()).or_insert(0);
            for depend in node.depends.iter().flatten() {
                *in_degree.entry(depend.clone()).or_insert(0) += 1;
            }
        }

        // INVARIANT: Queue nodes with in-degree of zero, i.e., nodes with no incoming edges.
        for (name, degree) in &in_degree {
            if *degree == 0 {
                queue.push_back(name.clone());
            }
        }

        // BFS terversal.
        while let Some(current) = queue.pop_front() {
            for depend in self.nodes[&current].depends.iter().flatten() {
                *in_degree.get_mut(depend).unwrap() -= 1;
                if *in_degree.get(depend).unwrap() == 0 {
                    queue.push_back(depend.clone());
                }
            }
            // INVARIANT: Mark each queued node as visited, representing toplogical sort of graph.
            visited.insert(current);
        }

        // INVARIANT: Queue is empty, but graph has not been fully visited.
        //   - There exists a cycle.
        //   - The unvisited nodes represent this cycle.
        if visited.len() != self.nodes.len() {
            let cycle: Vec<String> = self
                .nodes
                .keys()
                .filter(|key| !visited.contains(*key))
                .cloned()
                .collect();

            // TODO: Pretty print structure of cycle, besides printing names of problematic nodes.
            return Err(anyhow!("Cluster contains cycle(s): {cycle:?}"));
        }

        log::debug!("toplogical sort of cluster nodes: {visited:?}");

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

        let mut cluster = Self { root, nodes, document };
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
/// # Invariants
///
/// Nodes and their dependencies are acyclic.
#[derive(Debug)]
pub struct DependencyIter<'cluster> {
    graph: &'cluster HashMap<String, Node>,
    visited: HashSet<String>,
    stack: VecDeque<String>,
}

impl<'cluster> Iterator for DependencyIter<'cluster> {
    type Item = (&'cluster str, &'cluster Node);

    fn next(&mut self) -> Option<Self::Item> {
        // Stack-based DFS traversal.
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
