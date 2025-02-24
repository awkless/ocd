// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Configuration model.
//!
//! This module provides APIs to handle OCD's configuration data model. Thus, reading,
//! deserializing, and any other utilities required for managing and manipulating configuration
//! data for the OCD binary is directly stored right here somewhere.
//!
//! The following information provides a basic overview regarding important concepts and
//! organization of OCD's configuration model.
//!
//! > __NOTE__: OCD uses the TOML 1.0 data exchange format for all configuration files.
//!
//! ## Clusters
//!
//! The OCD tool operates on a __cluster__. A _cluster_ is a collection of Git repositories that can
//! be deployed together. The cluster is comprised of three repository types: __normal__,
//! __bare-alias__, __root__.
//!
//! A _normal_ repository is just a regular Git repository whose gitdir and worktree point to the
//! same path.
//!
//! A _bare-alias_ repository is a bare Git repository that uses a target directory as an alias of a
//! worktree. That target directory can be treated like a Git repository without initilization.
//!
//! A _root_ repository is a special bare-alias Git repository. It represents the root of the
//! cluster. It is responsible for containing the configuration data that defines the cluster
//! itself. A cluster can only have _one_ root repository.
//!
//! The concept of a cluster provides the user with a lot of flexibility in how they choose to
//! organize their dotfile configurations. The user can store dotfiles in separate repositories and
//! plug them into a given cluster whenever they want. The user can also maintain a monolithic
//! repository containing every possible configuration file they want if they so choose.
//!
//! See the [`Cluster`] type for what a cluster definition looks like.

mod cluster;

#[doc(inline)]
pub use cluster::*;

use anyhow::{anyhow, Result};
use mkdirp::mkdirp;
use std::path::{Path, PathBuf};

/// Configuration layout handler.
///
/// Responsible for determining and setting up common paths that the OCD binary uses to store
/// important configuration data that it needs to have access to on the user's file system.
#[derive(Debug, Clone)]
pub struct Layout {
    home_dir: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl Layout {
    /// Construct new configuration layout handler.
    ///
    /// This method will determine user's home directory, and OCD's common configuration data
    /// paths. This method will also construct any important directory path if it does not already
    /// exist, with exception for the user's home directory.
    ///
    /// ## Errors
    ///
    /// - Will fail if user's home directory cannot be determined.
    /// - Will fail if OCD's configuration directory cannot be determined.
    /// - Will fail if OCD's data directory cannot be determined.
    /// - Will fail if any important directory path cannot be constructed.
    pub fn new() -> Result<Self> {
        let home_dir = dirs::home_dir().ok_or(anyhow!("Cannot determine home directory"))?;
        let config_dir = dirs::config_dir()
            .ok_or(anyhow!("Cannot determine config directory"))?
            .join("ocd");
        let data_dir = dirs::data_dir()
            .ok_or(anyhow!("Cannot determine data directory"))?
            .join("ocd");

        mkdirp(&config_dir)?;
        mkdirp(&data_dir)?;

        Ok(Self {
            home_dir,
            config_dir,
            data_dir,
        })
    }

    /// Provide absolute [`Path`] to user's home directory.
    pub fn home_dir(&self) -> &Path {
        &self.home_dir
    }

    /// Provide absolute [`Path`] to configuration directory.
    ///
    /// Typically used to store important configuration files for OCD.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Provide absolute [`Path`] to data directory.
    ///
    /// Typically used to store repositories that OCD needs to manage and manipulate.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}
