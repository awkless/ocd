// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Configuration model.
//!
//! Handles the parsing, deserialization, and overall management of configuration file data for the
//! OCD tool.

pub mod cluster;
pub mod hook;

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tracing::{instrument, warn};

/// Get absolute path to user's home directory.
///
/// # Errors
///
/// - Will fail if user's home directory cannot be determined.
pub fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or(anyhow!("Cannot determine path to home directory"))
}

/// Get absolute path to OCD's standard configuration directory.
///
/// # Invariants
///
/// - OCD's standard configuration directory is always relative to user's home directory.
///
/// # Errors
///
/// - Will fail if user's home directory cannot be determined.
pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|path| path.join("ocd"))
        .ok_or(anyhow!("Cannot determine path to configuration directory"))
}

/// Get absolute path to OCD's data directory.
///
/// # Invariants
///
/// - OCD's standard data directory is always relative to user's home directory.
///
/// # Errors
///
/// - Will fail if user's home directory cannot be determined.
pub fn data_dir() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|path| path.join("ocd"))
        .ok_or(anyhow!("Cannot determine path to data directory"))
}

/// Use Unix-like glob pattern matching.
///
/// Will match a set of patterns to a given set of entries. Whatever is matched is returned as a
/// new vector to operate with. Invalid patterns or patterns with no matches or excluded from the
/// new vector, and logged as errors.
///
/// # Invariants
///
/// - Always produce valid vector containing matched entries only.
/// - Process full pattern list without failing.
#[instrument(skip(patterns, entries), level = "debug")]
pub(crate) fn glob_match(
    patterns: impl IntoIterator<Item = impl Into<String>> + std::fmt::Debug,
    entries: impl IntoIterator<Item = impl Into<String>> + std::fmt::Debug,
) -> Vec<String> {
    let patterns = patterns.into_iter().map(Into::into).collect::<Vec<String>>();
    let entries = entries.into_iter().map(Into::into).collect::<Vec<String>>();

    let mut matched = Vec::new();
    for pattern in &patterns {
        let pattern = match glob::Pattern::new(pattern) {
            Ok(pattern) => pattern,
            Err(error) => {
                warn!("Invalid pattern {pattern}: {error}");
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
            warn!("Pattern {} does not match any entries", pattern.as_str());
        }
    }

    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq as pretty_assert_eq;
    use simple_test_case::test_case;

    #[test_case(
        vec!["*sh".into(), "[f-g]oo".into(), "d?o".into()],
        vec!["sh".into(), "bash".into(), "foo".into(), "goo".into(), "doo".into()],
        vec!["sh".into(), "bash".into(), "foo".into(), "goo".into(), "doo".into()];
        "match all"
    )]
    #[test_case(
        vec!["foo".into(), "bar".into()],
        vec!["vim".into(), "dwm".into(), "sh".into()],
        Vec::<String>::new();
        "no match"
    )]
    #[test_case(
        vec!["[1-".into(), "[!a-d".into()],
        vec!["vim".into(), "dwm".into(), "sh".into()],
        Vec::<String>::new();
        "invalid pattern"
    )]
    #[test]
    fn smoke_glob_match(patterns: Vec<String>, entries: Vec<String>, mut expect: Vec<String>) {
        let mut result = glob_match(patterns, entries);
        expect.sort();
        result.sort();
        pretty_assert_eq!(result, expect);
    }
}
