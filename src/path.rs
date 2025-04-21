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
