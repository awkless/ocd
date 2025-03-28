// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

//! File system I/O.
//!
//! This module provides utilities to manage file system I/O operations for the OCD tool. Reading,
//! writing, and determining valid paths for file data itself is all provided here.

use anyhow::{anyhow, Result};
use mkdirp::mkdirp;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

/// Determine absolute paths to required dirctories.
///
/// The OCD tool needs to know the absolute paths for the user's home directory, configuration
/// directory, and data directory. The home directory is often used as the default location when
/// the user does not define a worktree alias to use for node's and root of a cluster. The
/// configuration directory is where OCD will locate configuration file data that it needs to
/// operate with. This configuration directory is currently expected to be in
/// `$XDG_CONFIG_HOME/ocd`. Finally, the data directory is where all deployable node repositories
/// will be stored, which is currently expected to be in `$XDG_DATA_HOME/ocd`.
#[derive(Debug, Clone)]
pub struct DirLayout {
    home: PathBuf,
    config: PathBuf,
    data: PathBuf,
}

impl DirLayout {
    /// Construct new directory layout paths.
    ///
    /// Will construct paths to configuration and data directory if they do not already exist.
    ///
    /// ## Errors
    ///
    /// Will fail if home directory, configuration directory, or data directory cannot be
    /// determined for whatever reason. Will also fail if configuration or data directories cannot
    /// be constructed when needed.
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or(anyhow!("Cannot find home directory"))?;
        let config = dirs::config_dir().ok_or(anyhow!("Cannot find config directory"))?.join("ocd");
        let data = dirs::data_dir().ok_or(anyhow!("Cannot find data directory"))?.join("ocd");

        mkdirp(&config)?;
        mkdirp(&data)?;

        Ok(Self { home, config, data })
    }

    /// Path to home directory.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Path to configuration directory.
    pub fn config(&self) -> &Path {
        &self.config
    }

    /// Path to data directory.
    pub fn data(&self) -> &Path {
        &self.data
    }
}

/// Read and deserialize contents of target configuration file.
///
/// General helper that uses type annotations to determine the correct type to parse and deserialize
/// a given file target to. Will create an empty configuration file if it does not already exist.
///
/// ## Errors
///
/// Will fail if path cannot be opened, created, or read. Will also fail if read string data cannot
/// be parsed and deserialized into target type..
pub fn read_config<C>(path: impl AsRef<Path>, dirs: &DirLayout) -> Result<C>
where
    C: std::str::FromStr<Err = anyhow::Error>,
{
    let full_path = dirs.config().join(path.as_ref());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(false)
        .read(true)
        .create(true)
        .open(full_path)?;

    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;

    let config: C = buffer.parse()?;
    Ok(config)
}
