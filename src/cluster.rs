// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Cluster configuration.
//!
//! This modules provides basic API to handle the parsing and deserialization of cluster
//! configurations.
//!
//! The OCD tool operates on a __cluster__. A _cluster_ is a collection of Git repositories that can
//! be deployed together. The cluster is comprised of three repository types: __normal__,
//! __bare-alias__, __root__.
//!
//! A _normal_ repository is just a regular Git repository whose gitdir and worktree point to the
//! same path.
//!
//! A _bare-alias_ repository is a bare Git repository that uses a target directory as an alias of a
//! worktree. That target directory can be treated like a Git repository without initilzation.
//!
//! A _root_ repository is a special bare-alias Git repository. It represents the root of the
//! cluster. It is responsible for containing the configuration data that defines the cluster
//! itself. A cluster can only have _one_ root repository.

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
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Cluster {
    /// Path to target directory to use as worktree alias for root.
    pub worktree: Option<PathBuf>,

    /// List of files to exclude from checkout of root.
    pub excludes: Option<Vec<String>>,

    /// Set of repository entries in cluster.
    pub node: HashMap<String, Node>,
}

impl Cluster {
    /// Iterate through dependencies of a node.
    pub fn dependency_iter(&self, node: impl Into<String>) -> DependencyIter<'_> {
        let mut stack = VecDeque::new();
        stack.push_front(node.into());

        DependencyIter {
            graph: &self.node,
            visited: HashSet::new(),
            stack,
        }
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

        Ok(())
    }

    pub fn expand_worktrees(&mut self) -> Result<()> {
        if let Some(worktree) = &self.worktree {
            self.worktree = Some(
                shellexpand::full(worktree.to_string_lossy().as_ref())
                    .with_context(|| "Failed to expand root worktree")?
                    .into_owned()
                    .into(),
            );
        }

        for (_, node) in self.node.iter_mut() {
            if let Some(worktree) = &self.worktree {
                node.worktree = Some(
                    shellexpand::full(worktree.to_string_lossy().as_ref())
                        .with_context(|| "Failed to expand root worktree")?
                        .into_owned()
                        .into(),
                );
            }
        }

        Ok(())
    }
}

impl str::FromStr for Cluster {
    type Err = anyhow::Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        toml::from_str::<Cluster>(data).with_context(|| "Failed to parse cluster")
    }
}

/// Structure of repository entry in cluster.
///
/// OCD refers to entries in a given cluster as _nodes_. Nodes can define other nodes as
/// dependencies for deployment.
///
/// # Invariant
///
/// Nodes must be acircular.
#[derive(Debug, Default, Deserialize, PartialEq, Eq, Hash)]
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
    type Item = &'cluster Node;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = self.stack.pop_front() {
            let node = &self.graph[&node];
            for depend in node.depends.iter().flatten() {
                if !self.visited.contains(depend) {
                    self.stack.push_front(depend.clone());
                    self.visited.insert(depend.clone());
                }
            }
            return Some(node);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::{fixture, rstest};
    use sealed_test::prelude::*;

    #[fixture]
    fn cluster_config() -> String {
        r#"
            worktree = "$HOME/ocd"
            excludes = ["README*", "LICENSE*"]

            [node.sh]
            url = "git@example.org:~user/sh.git"
            bare_alias = true
            worktree = "$HOME"
            excludes = ["README*", "LICENSE*"]

            [node.shell_alias]
            url = "git@example.org:~user/shell_alias.git"
            bare_alias = true
            worktree = "$HOME"
            excludes = ["README*", "LICENSE*"]

            [node.bash]
            url = "git@example.org:~user/bash.git"
            bare_alias = true
            worktree = "$HOME"
            excludes = ["README*", "LICENSE*"]
            depends = ["sh", "shell_alias"]

            [node.dwm]
            url = "git@example.org:~user/dwm.git"
            bare_alias = false
        "#
        .to_string()
    }

    #[rstest]
    fn cluster_from_str_accept_str(cluster_config: String) -> Result<()> {
        cluster_config.parse::<Cluster>()?;
        Ok(())
    }

    #[rstest]
    fn cluster_from_str_reject_str(
        #[values("[fail # here", "not.gonna = [work]", "bad + snafu")] input: &str,
    ) {
        let cluster: Result<Cluster> = input.parse();
        assert!(cluster.is_err());
    }

    #[rstest]
    #[case::deps(
        "bash",
        vec![
            Node {
                url: "git@example.org:~user/sh.git".into(),
                bare_alias: true,
                worktree: Some("$HOME".into()),
                excludes: Some(vec!["README*".into(), "LICENSE*".into()]),
                ..Default::default()
            },
            Node {
                url: "git@example.org:~user/shell_alias.git".into(),
                bare_alias: true,
                worktree: Some("$HOME".into()),
                excludes: Some(vec!["README*".into(), "LICENSE*".into()]),
                ..Default::default()
            },
            Node {
                url: "git@example.org:~user/bash.git".into(),
                bare_alias: true,
                worktree: Some("$HOME".into()),
                excludes: Some(vec!["README*".into(), "LICENSE*".into()]),
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
    fn cluster_dependency_iter_works(
        cluster_config: String,
        #[case] node: &str,
        #[case] expect: Vec<Node>,
    ) -> Result<()> {
        let cluster: Cluster = cluster_config.parse()?;
        let result: HashSet<&Node> = cluster.dependency_iter(node).collect();
        assert!(expect.iter().all(|node| result.contains(&node)));
        Ok(())
    }

    #[rstest]
    #[case(
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
        "#
    )]
    #[case(
        r#"
            [node.foo]
            url = "git@example.org:~user/foo.git"
            bare_alias = true
            depends = ["foo"]
        "#
    )]
    #[case(
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
        "#
    )]
    fn cluster_cycle_check_catches_cycles(#[case] input: &str) -> Result<()> {
        let cluster: Cluster = input.parse()?;
        let result = cluster.cycle_check();
        assert!(result.is_err());
        Ok(())
    }

    #[rstest]
    #[case(
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
        "#
    )]
    #[case(
        r#"
            [node.foo]
            url = "git@example.org:~user/foo.git"
            bare_alias = true
        "#
    )]
    #[case(
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
        "#
    )]
    fn cluster_cycle_check_accepts_acyclic_graph(#[case] input: &str) -> Result<()> {
        let cluster: Cluster = input.parse()?;
        let result = cluster.cycle_check();
        assert!(result.is_ok());
        Ok(())
    }

    #[rstest]
    #[sealed_test(env = [("HOME", "/some/path")])]
    fn cluster_expand_worktrees(cluster_config: String) -> Result<()> {
        let mut cluster: Cluster = cluster_config.parse()?;
        cluster.expand_worktrees()?;

        assert_eq!(cluster.worktree, Some(PathBuf::from("/some/path/ocd")));
        for (_, node) in cluster.node.iter() {
            assert_eq!(node.worktree, Some(PathBuf::from("/some/path/ocd")));
        }

        Ok(())
    }
}
