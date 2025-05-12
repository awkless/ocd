// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Configuration model.
//!
//! Handles the parsing, deserialization, and overall management of configuration file data for the
//! OCD tool.

use anyhow::{anyhow, Result};
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{fmt, marker::PhantomData, path::PathBuf, str::FromStr};

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
    D: Deserializer<'de>,
{
    let result: String = Deserialize::deserialize(deserializer)?;
    match result.as_str() {
        "config_dir" => Ok(WorkDirAlias::new(config_dir().map_err(serde::de::Error::custom)?)),
        "home_dir" => Ok(WorkDirAlias::new(home_dir().map_err(serde::de::Error::custom)?)),
        _ => Err(anyhow!("Invalid deployment option for root")).map_err(serde::de::Error::custom),
    }
}

/// Node entry of cluster.
///
/// A cluster typically contains a series of nodes. A given node entry can either be normal or
/// bare-alias. If the user does not specify a working directory alias, then their home directory
/// will be used as the default. All nodes contain a URL that points to a remote repository. This
/// URL is mainly used to tell OCD where to clone the node itself if it is missing in the
/// repository store.
///
/// Node entries have access to the file exclusion feature, and dependency deployment feature.
/// Thus, each node can have a listing of sparsity rules to exclude files and directories from
/// deployment, and a listing of other nodes as dependencies that must be deployed with the node
/// itself.
#[derive(Debug, PartialEq, Eq, Deserialize)]
pub struct NodeEntry {
    /// Deployment method for node entry.
    #[serde(deserialize_with = "deserialize_node_deployment")]
    pub deployment: NodeDeployment,

    /// URL to clone node entry from.
    pub url: String,

    /// List of sparisty rules to exclude files from deployment.
    pub excluded: Option<Vec<String>>,

    /// List of other nodes to be deployed as dependencies with this node entry.
    pub dependencies: Option<Vec<String>>,
}

/// Node deployment method.
///
/// Currently, there are only two kinds of node deployment:
///
/// 1. Normal deployment kind.
/// 2. Bare-alias deployment kind.
///
/// Normal deployment simply ensures that the node entry has been cloned into repository store.
/// Bare-alias deployment not only ensures that node entry has been cloned into repository store,
/// but is also properly deployed to target working directory alias.
#[derive(Debug, PartialEq, Eq, Deserialize)]
pub struct NodeDeployment {
    /// Deployment kind.
    pub kind: DeploymentKind,

    /// Working directory alias to use.
    pub work_dir_alias: WorkDirAlias,
}

impl FromStr for NodeDeployment {
    type Err = anyhow::Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let (kind, work_dir_alias) = match data {
            "normal" => (DeploymentKind::Normal, WorkDirAlias::default()),
            "bare_alias" => (DeploymentKind::BareAlias, WorkDirAlias::new(home_dir()?)),
            _ => return Err(anyhow!("Invalid deployment kind")),
        };

        Ok(NodeDeployment { kind, work_dir_alias })
    }
}

struct NodeDeploymentVisitor(PhantomData<fn() -> NodeDeployment>);

impl<'de> Visitor<'de> for NodeDeploymentVisitor {
    type Value = NodeDeployment;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string or map")
    }

    fn visit_str<E>(self, value: &str) -> Result<NodeDeployment, E>
    where
        E: serde::de::Error,
    {
        FromStr::from_str(value).map_err(serde::de::Error::custom)
    }

    fn visit_map<M>(self, map: M) -> Result<NodeDeployment, M::Error>
    where
        M: MapAccess<'de>,
    {
        Deserialize::deserialize(serde::de::value::MapAccessDeserializer::new(map))
    }
}

fn deserialize_node_deployment<'de, D>(deserializer: D) -> Result<NodeDeployment, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(NodeDeploymentVisitor(PhantomData))
}

/// Variants of node deployment.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum DeploymentKind {
    /// Node is normal, so make sure it got cloned.
    Normal,

    /// Node is bare-alias, make sure it got cloned, and is deployed to working directory alias.
    BareAlias,
}

/// Working directory alias path.
#[derive(Debug, Default, PartialEq, Eq, Deserialize)]
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

    #[test_case(
        r#"
            deployment = "normal"
            url = "https://some/url"
        "#,
        NodeEntry  {
            deployment: NodeDeployment {
                kind: DeploymentKind::Normal,
                work_dir_alias: WorkDirAlias::default(),
            },
            url: "https://some/url".into(),
            excluded: None,
            dependencies: None
        };
        "str_normal"
    )]
    #[test_case(
        r#"
            deployment = "bare_alias"
            url = "https://some/url"
        "#,
        NodeEntry  {
            deployment: NodeDeployment {
                kind: DeploymentKind::BareAlias,
                work_dir_alias: WorkDirAlias::new("some/path"),
            },
            url: "https://some/url".into(),
            excluded: None,
            dependencies: None
        };
        "str_bare_alias"
    )]
    #[test_case(
        r#"
            deployment = { kind = "normal", work_dir_alias = "blah/blah" }
            url = "https://some/url"
        "#,
        NodeEntry  {
            deployment: NodeDeployment {
                kind: DeploymentKind::Normal,
                work_dir_alias: WorkDirAlias::new("blah/blah"),
            },
            url: "https://some/url".into(),
            excluded: None,
            dependencies: None
        };
        "map_normal"
    )]
    #[test_case(
        r#"
            deployment = { kind = "bare_alias", work_dir_alias = "blah/blah" }
            url = "https://some/url"
        "#,
        NodeEntry  {
            deployment: NodeDeployment {
                kind: DeploymentKind::BareAlias,
                work_dir_alias: WorkDirAlias::new("blah/blah"),
            },
            url: "https://some/url".into(),
            excluded: None,
            dependencies: None
        };
        "map_bare_alias"
    )]
    #[sealed_test(env = [("HOME", "some/path"), ("XDG_CONFIG_HOME", "some/path/.config")])]
    fn node_entry_valid_deployment(config: &str, expect: NodeEntry) -> Result<()> {
        let node: NodeEntry = toml::de::from_str(config)?;
        pretty_assert_eq!(node, expect);
        Ok(())
    }

    #[test_case(
        r#"
            deployment = "snafu"
            url = "https://some/url"
        "#;
        "invalid_str"
    )]
    #[test_case(
        r#"
            deployment = { kind = "snafu", work_dir_alias = "blah/blah" }
            url = "https://some/url"
        "#;
        "unknown_field"
    )]
    fn node_entry_invalid_deployment(config: &str) {
        let result: Result<NodeEntry> = toml::de::from_str(config).with_context(|| "should fail!");
        assert!(result.is_err());
    }
}
