// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Data model types.
//!
//! Contains various types that represent, and help manipulate OCD's data model. Currently, the
//! [`Cluster`] type is provided as a format preserving cluster definition parser.

use crate::{
    path::{config_dir, home_dir},
    Error, Result,
};

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Key, Table, Value};
use tracing::{debug, instrument};

/// Format preserving cluster definition parser.
///
/// Obtains valid parsing of user's cluster definition in deserialized form. Provides additional
/// utilities to make it easer to extract and serialize cluster data for further manipulation. This
/// type only operates on strings. Caller is responsible for file I/O.
///
/// # Invariants
///
/// - Node dependencies exist in cluster.
/// - Node dependencies are acyclic.
/// - Directory aliases are always expanded.
#[derive(Clone, Default, Debug)]
pub struct Cluster {
    /// Root of cluster definition.
    pub root: RootEntry,

    /// All node entries in cluster definition represented as DAG.
    pub nodes: HashMap<String, NodeEntry>,

    document: DocumentMut,
}

impl Cluster {
    /// Construct new empty cluster definition.
    pub fn new() -> Self {
        Cluster::default()
    }

    /// Add node entry into cluster definition.
    ///
    /// Will replace existing node entry if given node entry was not new, returning the old entry
    /// that was replaced. Will construct a new "nodes" table if it does not already exist.
    ///
    /// # Errors
    ///
    /// - Return [`Error::EntryNotTable`] if "nodes" was defined, but not as a table as expected.
    ///
    /// [`Error::EntryNotTable`]: crate::Error::EntryNotTable
    pub fn add_node(
        &mut self,
        name: impl AsRef<str>,
        node: NodeEntry,
    ) -> Result<Option<NodeEntry>> {
        let (key, item) = node.to_toml(name.as_ref());
        let table = if let Some(item) = self.document.get_mut("nodes") {
            item.as_table_mut().ok_or(Error::EntryNotTable {
                name: "nodes".into(),
            })?
        } else {
            // INVARIANT: Construct new "nodes" table to insert node entry into.
            let mut new_table = Table::new();
            new_table.set_implicit(true);
            self.document.insert("nodes", Item::Table(new_table));
            // Will not panic since we just inserted the "nodes" table beforehand.
            self.document["nodes"].as_table_mut().unwrap()
        };

        // TODO: Transfer comments and whitespace of old entry to new entry that replaced it.
        //   - Is this really worth doing?
        table.insert_formatted(&key, item);

        Ok(self.nodes.insert(name.as_ref().into(), node))
    }

    /// Remove existing node entry from cluster definition.
    ///
    /// # Errors
    ///
    /// - Return [`Error::EntryNotFound`] if "nodes" table or node entry itself cannot be found.
    ///
    /// [`Error::EntryNotFound`]: crate::Error::EntryNotFound
    pub fn remove_node(&mut self, node: impl AsRef<str>) -> Result<NodeEntry> {
        self.document
            .get_mut("nodes")
            .and_then(|item| item.as_table_mut())
            .ok_or(Error::EntryNotFound {
                name: "nodes".into(),
            })?
            .remove(node.as_ref())
            .ok_or(Error::EntryNotFound {
                name: node.as_ref().into(),
            })?;

        self.nodes
            .remove(node.as_ref())
            .ok_or(Error::EntryNotFound {
                name: node.as_ref().into(),
            })
    }

    /// Iterate through all dependencies of a target node entry.
    ///
    /// Provides full path through each dependency of target node inclusively.
    pub fn dependency_iter(&self, node: impl Into<String>) -> DependencyIter<'_> {
        let mut stack = VecDeque::new();
        stack.push_front(node.into());
        DependencyIter {
            graph: &self.nodes,
            visited: HashSet::new(),
            stack,
        }
    }

    fn dependency_existence_check(&self) -> Result<()> {
        let mut results = Vec::new();
        for node in self.nodes.values() {
            for dependency in node.dependencies.iter().flatten() {
                if !self.nodes.contains_key(dependency) {
                    results.push(Err(Error::DependencyNotFound {
                        name: dependency.clone(),
                    }));
                } else {
                    results.push(Ok(()));
                }
            }
        }

        results.into_iter().collect::<_>()
    }

    #[instrument(skip(self))]
    fn acyclic_check(&self) -> Result<()> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        // INVARIANT: The in-degree of each node is the sum of all incoming edges to each
        // destination node.
        for (name, node) in &self.nodes {
            in_degree.entry(name.clone()).or_insert(0);
            for dependency in node.dependencies.iter().flatten() {
                *in_degree.entry(dependency.clone()).or_insert(0) += 1;
            }
        }

        // INVARIANT: Queue always contains nodes with in-degree of 0, i.e., nodes with no incoming
        // edges.
        for (name, degree) in &in_degree {
            if *degree == 0 {
                queue.push_back(name.clone());
            }
        }

        // BFS traversal such that the in-degree of all dependencies of a popped node from queue is
        // decremented by one. If a given dependency's in-degree becomes zero, push it into the
        // queue to be traversed. Finally, mark the currently popped node as visisted.
        while let Some(current) = queue.pop_front() {
            for dependency in self.nodes[&current].dependencies.iter().flatten() {
                *in_degree.get_mut(dependency).unwrap() -= 1;
                if *in_degree.get(dependency).unwrap() == 0 {
                    queue.push_back(dependency.clone());
                }
            }
            // INVARIANT: Visited nodes represent the topological sort of graph.
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
            return Err(Error::CircularDependencies { cycle });
        }

        debug!("Topological sort of cluster nodes: {visited:?}");

        Ok(())
    }

    fn expand_dir_aliases(&mut self) -> Result<()> {
        self.root.dir_alias =
            DirAlias::new(shellexpand::full(&self.root.dir_alias.to_string())?.into_owned());

        for node in self.nodes.values_mut() {
            if let DeploymentKind::BareAlias(dir_alias) = &node.deployment {
                node.deployment = DeploymentKind::BareAlias(DirAlias::new(
                    shellexpand::full(&dir_alias.to_string())?.into_owned(),
                ));
            }
        }

        Ok(())
    }
}

impl std::fmt::Display for Cluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.document)
    }
}

impl std::str::FromStr for Cluster {
    type Err = Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let document: DocumentMut = data.parse()?;
        let root = RootEntry::try_from(document.as_table())?;
        let nodes = if let Some(entries) = document.get("nodes").and_then(|node| node.as_table()) {
            let mut nodes: HashMap<String, NodeEntry> = HashMap::new();
            for (key, value) in entries.iter() {
                nodes.insert(key.into(), NodeEntry::try_from(value)?);
            }
            nodes
        } else {
            HashMap::new()
        };

        let mut cluster = Self {
            root,
            nodes,
            document,
        };
        cluster.dependency_existence_check()?;
        cluster.acyclic_check()?;
        cluster.expand_dir_aliases()?;

        Ok(cluster)
    }
}

/// Iterator for generating valid node entry dependency paths.
///
/// # Invariants
///
/// Nodes and their dependencies are acyclic.
#[derive(Debug)]
pub struct DependencyIter<'cluster> {
    graph: &'cluster HashMap<String, NodeEntry>,
    visited: HashSet<String>,
    stack: VecDeque<String>,
}

impl<'cluster> Iterator for DependencyIter<'cluster> {
    type Item = (&'cluster str, &'cluster NodeEntry);

    fn next(&mut self) -> Option<Self::Item> {
        // INVARIANT: Nodes and their dependencies are acyclic through acyclic check performed
        // during deserialization through `Cluster::from_str`.
        if let Some(node) = self.stack.pop_front() {
            let (name, node) = self.graph.get_key_value(&node)?;
            for dependency in node.dependencies.iter().flatten() {
                if !self.visited.contains(dependency) {
                    self.stack.push_front(dependency.clone());
                    self.visited.insert(dependency.clone());
                }
            }

            return Some((name.as_ref(), node));
        }

        None
    }
}

/// Root entry of cluster definition.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct RootEntry {
    /// Target directory to act as worktree alias for deployment.
    pub dir_alias: DirAlias,

    /// List of sparsity rules to exclude files from deployment.
    pub excluded: Option<Vec<String>>,
}

impl RootEntry {
    /// Construct new empty root entry.
    pub fn new() -> Self {
        RootEntry::default()
    }
}

impl<'toml> TryFrom<&'toml Table> for RootEntry {
    type Error = Error;

    /// Try to deserialize TOML table to [`RootEntry`].
    ///
    /// If field `dir_alias` is not defined, then it will default to using OCD's configuration
    /// directory path.
    ///
    /// # Errors
    ///
    /// - Return [`Error::NoWayConfig`] if OCD's configuration directory path could not be
    ///   determined.
    ///
    /// [`Error::NoWayConfig`]: crate::Error::NoWayConfig
    fn try_from(table: &'toml Table) -> Result<Self, Self::Error> {
        let mut root = RootEntry::new();

        let dir_alias = table
            .get("dir_alias")
            .and_then(|alias| alias.as_str().map(Into::into))
            // INVARIANT: Default to configuration directory path if `None`.
            .unwrap_or(config_dir()?);
        root.dir_alias = DirAlias::new(dir_alias);

        root.excluded = table.get("excluded").and_then(|rules| {
            rules.as_array().map(|arr| {
                arr.into_iter()
                    .map(|rule| rule.as_str().unwrap_or_default().into())
                    .collect()
            })
        });

        Ok(root)
    }
}

/// Node entry for cluster configuration.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct NodeEntry {
    /// Method of deployment for node entry.
    pub deployment: DeploymentKind,

    /// URL to clone node entry from.
    pub url: String,

    /// List of sparsity rules to exclude files from deployment.
    pub excluded: Option<Vec<String>>,

    /// List of node dependencies to include for deployment.
    pub dependencies: Option<Vec<String>>,
}

impl NodeEntry {
    /// Construct new empty node entry.
    pub fn new() -> Self {
        NodeEntry::default()
    }

    pub fn to_toml(&self, name: impl AsRef<str>) -> (Key, Item) {
        let mut node = Table::new();

        match &self.deployment {
            DeploymentKind::Normal => {
                node.insert("deployment", Item::Value(Value::from("normal")));
            }
            DeploymentKind::BareAlias(alias) => {
                if alias.is_home_dir() {
                    node.insert("deployment", Item::Value(Value::from("bare_alias")));
                } else {
                    let mut inline = InlineTable::new();
                    inline.insert("kind", Value::from("bare_alias"));
                    inline.insert("dir_alias", Value::from(alias.to_string()));
                    node.insert("deployment", Item::Value(Value::from(inline)));
                }
            }
        }

        node.insert("url", Item::Value(Value::from(&self.url)));

        if let Some(excluded) = &self.excluded {
            node.insert(
                "excluded",
                Item::Value(Value::Array(Array::from_iter(excluded))),
            );
        }

        if let Some(dependencies) = &self.dependencies {
            node.insert(
                "dependencies",
                Item::Value(Value::Array(Array::from_iter(dependencies))),
            );
        }

        let key = Key::new(name.as_ref());
        let value = Item::Table(node);
        (key, value)
    }
}

impl<'toml> TryFrom<&'toml Item> for NodeEntry {
    type Error = Error;

    fn try_from(item: &'toml Item) -> Result<Self, Self::Error> {
        let mut node = NodeEntry::new();

        node.deployment = if let Some(deployment) = item.get("deployment") {
            // INVARIANT: Allow deserialization from `&str` value.
            //   - Accept "normal" as `DeploymentKind::Normal`.
            //   - Accept "bare_alias` as `DeploymentKind::BareAlias(..)` such that it falls back
            //     on user's home directory path as the default.
            //   - Default to `DeploymentKind::default` for any other `&str`.
            if let Some(entry) = deployment.as_str() {
                match entry {
                    "normal" => DeploymentKind::Normal,
                    "bare_alias" => DeploymentKind::BareAlias(DirAlias::new(home_dir()?)),
                    &_ => DeploymentKind::default(),
                }
            // INVARIANT: Allow deserialization from `&InlineTable`.
            //   - Accept "{ kind = "normal" }" as `DeploymentKind::Normal`.
            //   - Accept "{ kind = "bare_alias", dir_alias = "<path>" }" as
            //     `DeploymentKind::BareAlias(DirAlias(<path>))`.
            //   - Accept "{ kind = "bare_alias" }" by falling back on user's home directory path
            //     as the default for `DeploymentKind::BareAlias(..)`.
            //   - Default to `DeploymentKind::default` for any other `&str`.
            } else {
                let kind = deployment
                    .get("kind")
                    .and_then(|kind| kind.as_str())
                    .unwrap_or_default();
                let alias = deployment
                    .get("dir_alias")
                    .and_then(|alias| alias.as_str().map(Into::into))
                    .unwrap_or(home_dir()?);
                match kind {
                    "normal" => DeploymentKind::Normal,
                    "bare_alias" => DeploymentKind::BareAlias(DirAlias::new(alias)),
                    &_ => DeploymentKind::default(),
                }
            }
        // INVARIANT: Use `DeploymentKind::default` if "deployment" field was never defined.
        } else {
            DeploymentKind::default()
        };

        node.url = item
            .get("url")
            .and_then(|url| url.as_str().map(Into::into))
            .unwrap_or_default();

        node.excluded = item.get("excluded").and_then(|rules| {
            rules.as_array().map(|arr| {
                arr.into_iter()
                    .map(|rule| rule.as_str().unwrap_or_default().into())
                    .collect()
            })
        });

        node.dependencies = item.get("dependencies").and_then(|deps| {
            deps.as_array().map(|arr| {
                arr.into_iter()
                    .map(|dep| dep.as_str().unwrap_or_default().into())
                    .collect()
            })
        });

        Ok(node)
    }
}

/// The variants of node deployment.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum DeploymentKind {
    /// Just make sure node entry is cloned.
    #[default]
    Normal,

    /// Make sure node entry is cloned, and deployed or undeployed to directory alias.
    BareAlias(DirAlias),
}

/// Directory path to use as an alias for a worktree.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct DirAlias(pub(crate) PathBuf);

impl DirAlias {
    /// Construct new directory alias from given path.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Determine if directory alias if pointing to home directory path.
    pub(crate) fn is_home_dir(&self) -> bool {
        let home = match home_dir() {
            Ok(path) => path,
            Err(_) => return false,
        };

        if self.0 == home {
            return true;
        }

        false
    }
}

impl std::fmt::Display for DirAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;
    use sealed_test::prelude::*;
    use simple_test_case::test_case;

    #[test_case(
        r#"
            dir_alias = "/some/path"
            excluded = ["rule1", "rule2", "rule3"]
        "#,
        RootEntry {
            dir_alias: DirAlias::new("/some/path"),
            excluded: Some(vec!["rule1".into(), "rule2".into(), "rule3".into()]),
        };
        "full fields"
    )]
    #[test_case(
        "",
        RootEntry {
            dir_alias: DirAlias::new("/home/user/.config/ocd"),
            excluded: None,
        };
        "missing all fields"
    )]
    #[sealed_test(env = [("HOME", "/home/user"), ("XDG_CONFIG_HOME", "/home/user/.config")])]
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
            deployment = { kind = "bare_alias", dir_alias = "/some/path" }
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
                    deployment: DeploymentKind::BareAlias(DirAlias::new("/home/user")),
                    url: "https://some/url".into(),
                    dependencies: Some(vec!["prompt".into()]),
                    ..Default::default()
                }
            ),
            (
                "prompt".into(),
                NodeEntry {
                    deployment: DeploymentKind::BareAlias(DirAlias::new("/some/path")),
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
    #[sealed_test(env = [("HOME", "/home/user")])]
    fn smoke_cluster_from_str_node_deserialize(
        config: &str,
        mut expect: Vec<(String, NodeEntry)>,
    ) -> Result<()> {
        let cluster: Cluster = config.parse()?;
        let mut result = cluster
            .nodes
            .into_iter()
            .collect::<Vec<(String, NodeEntry)>>();
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

    #[sealed_test(env = [("CUSTOM_VAR", "/some/path")])]
    fn smoke_cluster_from_str_expand_dir_aliases() -> Result<()> {
        let config = r#"
            dir_alias = "$CUSTOM_VAR/ocd"

            [nodes.vim]
            deployment = { kind = "bare_alias", dir_alias = "$CUSTOM_VAR/vimrc" }

            [nodes.bash]
            deployment = { kind = "bare_alias", dir_alias = "$CUSTOM_VAR/bash" }

            [nodes.fish]
            deployment = { kind = "bare_alias", dir_alias = "$CUSTOM_VAR/fish" }
        "#;
        let cluster: Cluster = config.parse()?;
        let expect = RootEntry {
            dir_alias: DirAlias::new("/some/path/ocd"),
            ..Default::default()
        };
        pretty_assertions::assert_eq!(cluster.root, expect);

        let mut expect = vec![
            (
                "vim".to_string(),
                NodeEntry {
                    deployment: DeploymentKind::BareAlias(DirAlias::new("/some/path/vimrc")),
                    ..Default::default()
                },
            ),
            (
                "bash".to_string(),
                NodeEntry {
                    deployment: DeploymentKind::BareAlias(DirAlias::new("/some/path/bash")),
                    ..Default::default()
                },
            ),
            (
                "fish".to_string(),
                NodeEntry {
                    deployment: DeploymentKind::BareAlias(DirAlias::new("/some/path/fish")),
                    ..Default::default()
                },
            ),
        ];
        let mut result = cluster
            .nodes
            .into_iter()
            .collect::<Vec<(String, NodeEntry)>>();
        result.sort_by(|(a, _), (b, _)| a.cmp(b));
        expect.sort_by(|(a, _), (b, _)| a.cmp(b));
        pretty_assertions::assert_eq!(result, expect);

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
            deployment: DeploymentKind::BareAlias(DirAlias::new("/home/user")),
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
            deployment: DeploymentKind::BareAlias(DirAlias::new("/some/path")),
            url: "https://some/url".into(),
            ..Default::default()
        },
        indoc! {r#"
            # This comment should remain!
            [nodes.dwm]
            deployment = "normal"
            url = "https://some/url"

            [nodes.vim]
            deployment = { kind = "bare_alias", dir_alias = "/some/path" }
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
            deployment: DeploymentKind::BareAlias(DirAlias::new("/some/path")),
            url: "https://some/url".into(),
            ..Default::default()
        },
        indoc! {r#"
            [nodes.dwm]
            deployment = { kind = "bare_alias", dir_alias = "/some/path" }
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
            deployment: DeploymentKind::BareAlias(DirAlias::new("/some/path")),
            url: "https://some/url".into(),
            ..Default::default()
        },
        indoc! {r#"
            [nodes.dwm]
            deployment = { kind = "bare_alias", dir_alias = "/some/path" }
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
    #[sealed_test(env = [("HOME", "/home/user")])]
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
    fn smoke_cluster_dependency_iter(
        config: &str,
        mut expect: Vec<(&str, NodeEntry)>,
    ) -> Result<()> {
        let cluster: Cluster = config.parse()?;
        let mut result: Vec<(&str, NodeEntry)> = cluster
            .dependency_iter("bash")
            .map(|(name, node)| (name, node.clone()))
            .collect();

        result.sort_by(|(a, _), (b, _)| a.cmp(b));
        expect.sort_by(|(a, _), (b, _)| a.cmp(b));
        pretty_assertions::assert_eq!(result, expect);
        Ok(())
    }
}
