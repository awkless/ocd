// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Cross-platform path manipulation.
//!
//! Provide basic utilities to determine and manipulate important path data that OCD needs in order
//! to properly operate.

use crate::{Error, Result};

use std::{fs::create_dir_all, path::PathBuf};

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
    let path = dirs::config_dir()
        .map(|path| path.join("ocd"))
        .ok_or(Error::NoWayConfig)?;

    if !path.exists() {
        create_dir_all(&path)?;
    }

    Ok(path)
}

/// Get absolute path to OCD's data directory.
///
/// # Errors
///
/// - Return [`Error::NoWayData`] if path to configuration directory cannot be determined.
///
/// [`Error::NoWayData`]: crate::Error::NoWayData
pub fn data_dir() -> Result<PathBuf> {
    let path = dirs::data_dir()
        .map(|path| path.join("ocd"))
        .ok_or(Error::NoWayData)?;

    if !path.exists() {
        create_dir_all(&path)?;
    }

    Ok(path)
}
