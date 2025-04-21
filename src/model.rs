// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Data model types.
//!
//! Contains various types that represent, and help manipulate OCD's data model. Currently, the
//! [`Cluster`] type is provided as a format preserving cluster definition parser.

use std::{collections::HashMap, path::PathBuf};
use toml_edit::DocumentMut;

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
}

/// Root entry of cluster definition.
#[derive(Clone, Default, Debug)]
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

/// Node entry for cluster configuration.
#[derive(Clone, Default, Debug)]
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
