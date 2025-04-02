// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

//! General utilities.
//!
//! This module provides general miscellaneous utilities to make life easier. These utilities where
//! placed here either because they did not seem to fit the purpose of other modules, but were
//! still important to have around.

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

/// Use Unix-like glob pattern matching.
///
/// Will match a set of patterns to a given set of entries. Whatever is matched is returned as a
/// new vector to operate with.
///
/// ## Errors
///
/// Will fail if one of the given list of patterns is invalid.
pub fn glob_match(
    patterns: impl IntoIterator<Item = impl Into<String>>,
    entries: impl IntoIterator<Item = impl Into<String>>,
) -> Vec<String> {
    let patterns = patterns.into_iter().map(Into::into).collect::<Vec<String>>();
    let entries = entries.into_iter().map(Into::into).collect::<Vec<String>>();

    let mut matched = Vec::new();
    for pattern in &patterns {
        let pattern = match glob::Pattern::new(pattern) {
            Ok(pattern) => pattern,
            Err(error) => {
                log::error!("Invalid pattern {pattern:?}: {error}");
                continue;
            }
        };

        let mut found = false;
        for entry in &entries {
            if pattern.matches(entry) {
                found = true;
                matched.push(entry.to_string());
            }
        }

        if !found {
            log::error!("Pattern {} does not match any entries", pattern.as_str());
        }
    }

    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[track_caller]
    fn check_glob_match(
        patterns: impl IntoIterator<Item = impl Into<String>>,
        entries: impl IntoIterator<Item = impl Into<String>>,
        expect: impl IntoIterator<Item = impl Into<String>>,
    ) {
        let mut expect = expect.into_iter().map(Into::into).collect::<Vec<String>>();
        expect.sort();

        let mut result = glob_match(patterns, entries);
        result.sort();
        assert_eq!(result, expect);
    }

    #[test]
    fn smoke_glob_match() -> Result<()> {
        check_glob_match(["*"], ["foo", "bar", "baz"], ["foo", "bar", "baz"]);
        check_glob_match(["*sh"], ["sh", "bash", "yash", "vim"], ["sh", "bash", "yash"]);
        check_glob_match(["vim", "foo"], ["foo", "dwm", "bar", "vim"], ["vim", "foo"]);
        check_glob_match(["foo", "bar"], ["vim", "dwm", "sh"], Vec::<String>::new());

        Ok(())
    }
}
