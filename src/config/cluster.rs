// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::{
    config::Layout,
    repo::{AliasDir, RepoKind},
};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    str,
};

/// Structure of a cluster configuration.
///
/// The root (top-level) table of configuration is used to configure the root repository that houses
/// this data.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Cluster {
    /// Path to target directory to use as worktree alias for root.
    pub worktree: Option<PathBuf>,

    /// List of files to exclude from checkout of root.
    pub excludes: Option<Vec<String>>,

    /// Set of repository entries in cluster.
    pub node: HashMap<String, Node>,
}

impl Cluster {
    /// Get specific node from cluster.
    ///
    /// # Errors
    ///
    /// Will fail if node does not exist in cluster.
    pub fn get_node(&self, name: impl AsRef<str>) -> Result<(&String, &Node)> {
        self.node.get_key_value(name.as_ref()).ok_or(anyhow!(
            "Node '{}' does not exist in cluster",
            name.as_ref()
        ))
    }

    /// Iterate through dependencies of a node.
    ///
    /// # Errors
    ///
    /// Will fail if start node does not exist in cluster.
    pub fn dependency_iter(&self, node: impl Into<String>) -> Result<DependencyIter<'_>> {
        let node = node.into();
        if !self.node.contains_key(&node) {
            return Err(anyhow!("Node '{node}' does not exist in cluster"));
        }

        let mut stack = VecDeque::new();
        stack.push_front(node);

        log::debug!("Iterate through dependencies of {}", stack.front().unwrap());
        Ok(DependencyIter {
            graph: &self.node,
            visited: HashSet::new(),
            stack,
        })
    }

    /// Check for a cycle between node dependencies.
    ///
    /// # Errors
    ///
    /// Will return names of the nodes preventing dependencies from being
    /// acyclic. List of names do not represent the full path of a cycle, nor
    /// paths for any sub-cycles. The names just tell the user that one or more
    /// cycles exist between them.
    pub fn cycle_check(&self) -> Result<()> {
        log::trace!("Circular dependency check");
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut count: usize = 0;
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        for (name, node) in self.node.iter() {
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
            for depend in self.node[&current].depends.iter().flatten() {
                *in_degree.get_mut(depend).unwrap() -= 1;
                if *in_degree.get(depend).unwrap() == 0 {
                    queue.push_back(depend.clone());
                }
            }
            visited.insert(current);
        }

        if count != self.node.len() {
            let cycle: Vec<String> = self
                .node
                .iter()
                .filter(|(name, _)| !visited.contains(*name))
                .map(|(name, _)| name.clone())
                .collect();
            return Err(anyhow!("Cluster contains cycle: {cycle:?}"));
        }
        log::trace!("No cycles found");

        Ok(())
    }

    /// Perform shell expansion on all available worktree fields.
    ///
    /// Will expand root worktree, and all node worktrees if available. The
    /// caller is responsible for handling the case where the root or a node
    /// does not have a worktree defined, i.e., [`None`].
    ///
    /// # Errors
    ///
    /// Will fail if worktree value cannot be shell expanded properly.
    pub fn expand_worktrees(&mut self) -> Result<()> {
        log::trace!("Expand worktree paths");
        if let Some(worktree) = &self.worktree {
            self.worktree = Some(
                shellexpand::full(worktree.to_string_lossy().as_ref())
                    .with_context(|| "Failed to expand root worktree")?
                    .into_owned()
                    .into(),
            );

            log::debug!(
                "Expand root worktree to {}",
                self.worktree.as_ref().unwrap().display()
            );
        }

        for (name, node) in self.node.iter_mut() {
            if let Some(worktree) = &node.worktree {
                node.worktree = Some(
                    shellexpand::full(worktree.to_string_lossy().as_ref())
                        .with_context(|| "Failed to expand root worktree")?
                        .into_owned()
                        .into(),
                );

                log::debug!(
                    "Expand node '{name}' worktree to {}",
                    node.worktree.as_ref().unwrap().display()
                );
            }
        }

        Ok(())
    }
}

impl str::FromStr for Cluster {
    type Err = anyhow::Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        log::trace!("Parse cluster data");
        let mut cluster: Cluster =
            toml::from_str(data).with_context(|| "Failed to parse cluster")?;
        cluster.cycle_check()?;
        cluster.expand_worktrees()?;
        Ok(cluster)
    }
}

/// Structure of repository entry in cluster.
///
/// OCD refers to entries in a given cluster as _nodes_. Nodes can define other nodes as
/// dependencies for deployment.
///
/// # Invariant
///
/// Node dependencies must be acircular.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq, Hash)]
pub struct Node {
    /// URL to remote to clone from.
    pub url: String,

    /// Flag to determine repository kind such that true is bare-alias, and false is normal.
    pub bare_alias: bool,

    /// Path to target directory to use a worktree alias (ignored if `bare_alias` flag is false).
    pub worktree: Option<PathBuf>,

    /// List of files to exclude on checkout.
    pub excludes: Option<Vec<String>>,

    /// List of other nodes in cluster as dependencies.
    pub depends: Option<Vec<String>>,
}

impl Node {
    /// Determine the kind of repository the current node is.
    pub fn repo_kind(&self, layout: &Layout) -> RepoKind {
        match self.bare_alias {
            true => {
                let path = self
                    .worktree
                    .as_ref()
                    .map(|p| p.as_ref())
                    .unwrap_or(layout.home_dir());
                RepoKind::BareAlias(AliasDir::new(path))
            }
            false => RepoKind::Normal,
        }
    }
}

/// Iterate through dependencies of given node.
///
/// Assumes that set of nodes are acyclic.
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
            log::debug!("Node dependency: {node}");
            let (name, node) = self.graph.get_key_value(&node).unwrap();
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

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::{anyhow, Result};
    use rstest::{fixture, rstest};
    use sealed_test::prelude::*;

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
            (
                "sh",
                Node {
                    url: "git@example.org:~user/sh.git".into(),
                    bare_alias: true,
                    worktree: Some("/some/path".into()),
                    ..Default::default()
                }
            ),
            (
                "shell_alias",
                Node {
                    url: "git@example.org:~user/shell_alias.git".into(),
                    bare_alias: true,
                    worktree: Some("/some/path".into()),
                    ..Default::default()
                }
            ),
            (
                "bash",
                Node {
                    url: "git@example.org:~user/bash.git".into(),
                    bare_alias: true,
                    worktree: Some("/some/path".into()),
                    excludes: None,
                    depends: Some(vec!["sh".into(), "shell_alias".into()]),
                }
            ),
        ],
    )]
    #[case::no_deps(
        "dwm",
        vec![
            (
                "dwm",
                Node {
                    url: "git@example.org:~user/dwm.git".into(),
                    ..Default::default()
                }
            )
        ],
    )]
    #[sealed_test(env = [("HOME", "/some/path")])]
    fn smoke_cluster_dependency_iter(
        config: String,
        #[case] node: &str,
        #[case] mut expect: Vec<(&str, Node)>,
    ) -> Result<()> {
        let cluster: Cluster = config.parse()?;
        let mut nodes = cluster
            .dependency_iter(node)?
            .collect::<Vec<(&str, &Node)>>();
        nodes.sort_by(|(a, _), (b, _)| a.to_lowercase().cmp(&b.to_lowercase()));
        expect.sort_by(|(a, _), (b, _)| a.to_lowercase().cmp(&b.to_lowercase()));
        for ((name1, node1), (name2, node2)) in expect.iter().zip(nodes.iter()) {
            assert_eq!(name1, name2);
            assert_eq!(&node1, node2);
        }
        Ok(())
    }
}
