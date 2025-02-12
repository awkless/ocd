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

use std::{path::PathBuf, collections::HashMap};
use serde::Deserialize;

/// Structure of a cluster configuration.
///
/// The root (top-level) table of configuration is used to configure the root repository that houses
/// this data.
#[derive(Debug, Deserialize)]
pub struct Cluster {
    /// Path to target directory to use as worktree alias for root.
    pub worktree: Option<String>,

    /// List of files to exclude from checkout of root.
    pub excludes: Option<Vec<String>>,

    /// Set of repository entries in cluster.
    pub node: HashMap<String, Node>,
}

/// Structure of repository entry in cluster.
///
/// OCD refers to entries in a given cluster as _nodes_. Nodes can define other nodes as
/// dependencies for deployment.
///
/// # Invariant
///
/// Nodes must be acircular.
#[derive(Debug, Deserialize)]
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

