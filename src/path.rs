// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Cross-platform path manipulation.
//!
//! Provide basic utilities to determine and manipulate important path data that OCD needs in order
//! to properly operate.

use crate::{Error, Result};

use std::path::PathBuf;

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
