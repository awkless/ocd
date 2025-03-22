// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

use anyhow::{Context, Result};
use std::path::PathBuf;
use toml_edit::{DocumentMut, Table};

#[derive(Default, Debug)]
pub struct Cluster {
    document: DocumentMut,
    root: Root,
}

impl Cluster {
    pub fn new() -> Self {
        Cluster::default()
    }

    pub fn get_root(&self) -> &Root {
        &self.root
    }
}

impl std::str::FromStr for Cluster {
    type Err = anyhow::Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let document: DocumentMut = data.parse().with_context(|| "Bad parse")?;
        let table = document.as_table();
        let root = Root::from(table);
        Ok(Self { document, root })
    }
}

impl std::fmt::Display for Cluster {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.document)
    }
}

#[derive(Default, Debug, Eq, PartialEq, Clone)]
pub struct Root {
    pub worktree: Option<PathBuf>,
    pub excludes: Option<Vec<String>>,
}

impl Root {
    pub fn new() -> Self {
        Root::default()
    }
}

impl<'toml> From<&'toml Table> for Root {
    fn from(table: &'toml Table) -> Self {
        let mut root = Root { ..Default::default() };
        root.worktree = table.get("worktree").and_then(|n| n.as_str().map(Into::into));
        root.excludes = table.get("excludes").and_then(|n| {
            n.as_array()
                .map(|a| a.into_iter().map(|s| s.as_str().unwrap_or_default().into()).collect())
        });
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_cluster_get_root() -> Result<()> {
        let toml = r#"
            worktree = "/some/path"
            excludes = ["file1", "file2"]

            [foo]
            worktree = "/blah/blah"
            excludes = ["ignore1", "ignore2"]

            [node.vim]
            worktree = ".vimrc"
            excludes = ["ftplugin", "*.bak"]
        "#;

        let cluster: Cluster = toml.parse()?;
        let result = cluster.get_root();
        let expect = Root {
            worktree: Some("/some/path".into()),
            excludes: Some(vec!["file1".into(), "file2".into()]),
        };
        assert_eq!(result, &expect);

        Ok(())
    }
}
