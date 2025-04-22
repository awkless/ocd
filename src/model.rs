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
use toml_edit::{DocumentMut, Item, Table};
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

        let cluster = Self {
            root,
            nodes,
            document,
        };
        cluster.acyclic_check()?;

        Ok(cluster)
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
    pub fn new() -> Self {
        NodeEntry::default()
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
