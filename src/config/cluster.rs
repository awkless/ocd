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
    pub fn get_node(&self, name: impl AsRef<str>) -> Result<(&String, &Node)> {
        self
            .node
            .get_key_value(name.as_ref())
            .ok_or(anyhow!("Node '{}' does not exist in cluster", name.as_ref()))
    }

    /// Iterate through dependencies of a node.
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
/// Nodes must be acircular.
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
