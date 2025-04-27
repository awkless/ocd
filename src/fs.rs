// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! File system interaction.
//!
//! This module provides basic utilitis for interacting with the user's file system. Nothing
//! special here.

use crate::{Error, Result};

use std::{
    fs::{create_dir_all, read_to_string, OpenOptions},
    io::Write,
    path::PathBuf,
};
use tracing::{debug, info, instrument};

/// Read configuration file and deserialize to target type.
///
/// Ignores non-existent configuration files if given [`Existence::NotRequired`].
///
/// # Errors
///
/// - Return `Error::Io` if file cannot be read.
/// - Return corresponding `Error` variant if deserialization to configuration type fails.
#[instrument(skip(filename), level = "debug")]
pub fn load<C>(filename: impl AsRef<str>, existence: Existence) -> Result<C>
where
    C: std::str::FromStr<Err = Error>,
{
    let config_dir = config_dir()?;
    if !config_dir.exists() {
        info!("create configuration directory at {config_dir:?}");
        create_dir_all(&config_dir)?;
    }

    let path = config_dir.join(filename.as_ref());
    debug!("Load configuration file {path:?}");

    let data = match read_to_string(path) {
        Ok(data) => Ok(data),
        Err(err) => {
            if existence == Existence::NotRequired {
                if err.kind() == std::io::ErrorKind::NotFound {
                    Ok(String::new())
                } else {
                    Err(err)
                }
            } else {
                Err(err)
            } }
    }?;

    data.parse::<C>()
}

/// Determine if existence of configuraiton file is a requirement.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Existence {
    /// Configuration file existence is a requirement, so fail if it cannot be found.
    #[default]
    Required,

    /// Configuratoin file existence is not a requirement, so do not fail if it cannot be found.
    NotRequired,
}

/// Serialize and write contents of configuration type to target file.
///
/// Will create the configuration file to write to, if it does not already exist. Overwrites
/// original content of target file.
///
/// # Errors
///
/// - Return `Error::Io` if file cannot be created or written to.
#[instrument(skip(filename, config), level = "debug")]
pub fn save<C>(filename: impl AsRef<str>, config: C) -> Result<()>
where
    C: std::fmt::Display,
{
    let config_dir = config_dir()?;
    if !config_dir.exists() {
        info!("create configuration directory at {config_dir:?}");
        create_dir_all(&config_dir)?;
    }

    let path = config_dir.join(filename.as_ref());
    debug!("Save configuration file {path:?}");

    OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)?
        .write_all(config.to_string().as_bytes())
        .map_err(Error::from)
}

/// Get absolute path to user's home directory.
///
/// # Errors
///
/// - Return [`Error::NoWayHome`] if path to home directory cannot be determined.
///
/// [`Error::NoWayHome`]: crate::Error::NoWayHome
pub fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or(Error::NoWayHome)
}

/// Get absolute path to OCD's configuration directory.
///
/// # Errors
///
/// - Return [`Error::NoWayConfig`] if path to configuration directory cannot be determined.
///
/// [`Error::NoWayConfig`]: crate::Error::NoWayConfig
pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|path| path.join("ocd"))
        .ok_or(Error::NoWayConfig)
}

/// Get absolute path to OCD's data directory.
///
/// # Errors
///
/// - Return [`Error::NoWayData`] if path to configuration directory cannot be determined.
///
/// [`Error::NoWayData`]: crate::Error::NoWayData
pub fn data_dir() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|path| path.join("ocd"))
        .ok_or(Error::NoWayData)
}
