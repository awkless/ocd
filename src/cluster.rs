// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

use anyhow::{Context, Result};
use std::path::PathBuf;
use toml_edit::{
    visit::{visit_table_like_kv, Visit},
    DocumentMut, Item, Table,
};

#[derive(Default, Debug)]
pub struct Cluster {
    document: DocumentMut,
}

impl Cluster {
    pub fn new() -> Self {
        Cluster::default()
    }

    pub fn get_root(&self) -> Root {
        let table = self.document.as_table();
        Root::from(table)
    }
}
impl std::str::FromStr for Cluster {
    type Err = anyhow::Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let document: DocumentMut = data.parse().with_context(|| "Bad parse")?;
        Ok(Self { document })
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
    state: VisitState,
}

impl Root {
    pub fn new() -> Self {
        Root::default()
    }
}

impl<'toml> From<&'toml Table> for Root {
    fn from(table: &'toml Table) -> Self {
        let mut root = Root { ..Default::default() };
        root.visit_table(table);
        root
    }
}

impl<'toml> Visit<'toml> for Root {
    fn visit_table_like_kv(&mut self, key: &'toml str, node: &'toml Item) {
        if self.state != VisitState::Root {
            self.state = self.state.descend(key);
            visit_table_like_kv(self, key, node);
        }

        if key == "worktree" {
            self.worktree = node.as_str().map(Into::into);
        }

        if key == "excludes" {
            self.excludes = node
                .as_array()
                .map(|a| a.into_iter().map(|s| s.as_str().unwrap_or_default().into()).collect());
        }
    }
}

#[derive(Copy, Default, Clone, Debug, Eq, PartialEq)]
enum VisitState {
    #[default]
    Root,
    Other,
}

impl VisitState {
    fn descend(self, key: &str) -> Self {
        match (self, key) {
            (VisitState::Root, "worktree" | "excludes") => VisitState::Root,
            (VisitState::Root | VisitState::Other, _) => VisitState::Other,
        }
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
            state: VisitState::default(),
        };
        assert_eq!(result, expect);

        Ok(())
    }
}
