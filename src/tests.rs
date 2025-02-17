// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

mod cluster;
mod vcs;

use anyhow::Result;
use git2::{Repository, RepositoryInitOptions};
use std::path::Path;

/// Git repository fixture.
///
/// Allows for the creation of a basic Git repository fixture for integration
/// testing. Allows user to create basic blobs in the index to test against.
/// Checkout is left to the caller to figure out.
pub(crate) struct RepoFixture {
    repo: Repository,
}

impl RepoFixture {
    pub fn init(path: impl AsRef<Path>, bare: bool) -> Result<Self> {
        let mut opts = RepositoryInitOptions::new();
        opts.bare(bare);
        opts.initial_head("master");
        opts.mkdir(true);
        opts.mkpath(true);

        let repo = Self {
            repo: Repository::init_opts(path.as_ref(), &opts)?,
        };
        repo.init_config()?;

        Ok(repo)
    }

    pub fn write_blob_then_commit(
        &self,
        path: impl AsRef<Path>,
        data: impl AsRef<str>,
    ) -> Result<()> {
        let blob_oid = self.repo.blob(data.as_ref().as_bytes())?;
        let mut tree_builder = self.repo.treebuilder(None)?;
        tree_builder.insert(path.as_ref(), blob_oid, 0o100644)?;
        let tree_oid = tree_builder.write()?;
        let tree = self.repo.find_tree(tree_oid)?;
        let signature = self.repo.signature()?;

        let mut parents = Vec::new();
        if let Some(parent) = self.repo.head().ok().map(|h| h.target().unwrap()) {
            parents.push(self.repo.find_commit(parent)?);
        }
        let parents = parents.iter().collect::<Vec<_>>();

        self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            format!("Add {}", path.as_ref().display()).as_str(),
            &tree,
            &parents,
        )?;
        Ok(())
    }

    fn init_config(&self) -> Result<()> {
        let mut config = self.repo.config()?;
        config.set_str("user.name", "John Doe")?;
        config.set_str("user.email", "john@doe.com")?;
        Ok(())
    }
}

/// Kind of repository to create.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum RepoFixtureKind {
    #[default]
    Normal,

    Bare,
}
