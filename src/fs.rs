// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! File system interaction.
//!
//! This module provides basic utilitis for interacting with the user's file system. Nothing
//! special here.

use crate::{Error, Result};

use std::{fs::{OpenOptions, read_to_string}, io::Write, path::Path};

/// Read configuration file and deserialize to target type.
///
/// Ignore missing configuration file.
///
/// # Errors
///
/// - Return `Error::Io` if file cannot be read.
/// - Return corresponding `Error` variant if deserialization to configuration type fails.
pub fn read_to_config<C>(path: impl AsRef<Path>) -> Result<C>
where
    C: std::str::FromStr<Err = Error>,
{
    let data = match read_to_string(path.as_ref()) {
        Ok(data) => Ok(data),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err),
    }?;

    data.parse::<C>()
}

/// Serialize and write contents of configuration type to target file.
///
/// Will create the configuration file to write to, if it does not already exist. Overwrites
/// original content of target file.
///
/// # Errors
///
/// - Return `Error::Io` if file cannot be created or written to.
pub fn write_to_config<C, P>(path: P, config: C) -> Result<()>
where
    C: std::fmt::Display,
    P: AsRef<Path>,
{
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path.as_ref())?
        .write_all(config.to_string().as_bytes())
        .map_err(Error::from)
}
