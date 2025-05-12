// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Configuration model.
//!
//! Handles the parsing, deserialization, and overall management of configuration file data for the
//! OCD tool.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Deserializer};
use std::path::PathBuf;

/// Root entry of cluster definition.
///
/// Any and all cluster's that OCD operates on must have a _root_. The root contains the cluster
/// definition, which must always be deployed in order for OCD to know what nodes it must manage
/// within the repository store.
///
/// Root is always bare-alias such that it can only be deployed at two locations relative to the
/// user's home directory:
///
/// 1. The user's home directory itself.
/// 2. The standard configuration directory for OCD, i.e., `$XDG_CONFIG_HOME/ocd`.
///
/// This restriction of deployment for root ensures that the cluster definition always exists at
/// the standard configuration directory. This ensures that the cluster definition can be reliably
/// read, parsed, and deserialized during runtime. This also ensures that OCD can easily clone a
/// target cluster and deploy it by simply using root itself.
///
/// Root also has access to the file exclusion feature. The user can specify a list of sparsity
/// rules to exclude certain files and directories from deployment.
#[derive(Debug, Deserialize)]
pub struct RootEntry {
    /// Working directory alias option.
    #[serde(deserialize_with = "deserialize_root_work_dir_alias")]
    pub work_dir_alias: WorkDirAlias,

    /// List of sparsity rules to exclude files from deployment.
    pub excluded: Option<Vec<String>>,
}

fn deserialize_root_work_dir_alias<'de, D>(deserializer: D) -> Result<WorkDirAlias, D::Error>
where
    D: Deserializer<'de, Error: >,
{
    let result: String = Deserialize::deserialize(deserializer)?;
    match result.as_str() {
        "config_dir" => Ok(WorkDirAlias::new(config_dir().map_err(serde::de::Error::custom)?)),
        "home_dir" => Ok(WorkDirAlias::new(home_dir().map_err(serde::de::Error::custom)?)),
        _ => Err(anyhow!("Invalid deployment option for root")).map_err(serde::de::Error::custom),
    }
}

/// Working directory alias path.
#[derive(Debug, PartialEq, Eq, Deserialize)]
pub struct WorkDirAlias(pub(crate) PathBuf);

impl WorkDirAlias {
    /// Construct new working directory alias based on provided path.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }
}

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
    dirs::config_dir().ok_or(anyhow!("Cannot determine path to configuration directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Context;
    use pretty_assertions::assert_eq as pretty_assert_eq;
    use sealed_test::prelude::*;
    use simple_test_case::test_case;

    #[test_case(r#"work_dir_alias = "home_dir""#, WorkDirAlias::new("some/path"); "home_dir")]
    #[test_case(r#"work_dir_alias = "config_dir""#, WorkDirAlias::new("some/path/.config"); "config_dir")]
    #[sealed_test(env = [("HOME", "some/path"), ("XDG_CONFIG_HOME", "some/path/.config")])]
    fn root_entry_valid_work_dir_alias(config: &str, expect: WorkDirAlias) -> Result<()> {
        let root: RootEntry = toml::de::from_str(config)?;
        pretty_assert_eq!(root.work_dir_alias, expect);
        Ok(())
    }

    #[test]
    fn root_entry_invalid_work_dir_alias() {
        let result: Result<RootEntry> =
            toml::de::from_str(r#"work_dir_alias = "data_dir""#).with_context(|| "should fail!");
        assert!(result.is_err());
    }
}
