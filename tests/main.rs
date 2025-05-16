// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

mod integration;

use anyhow::Result;
use git2::{IndexEntry, IndexTime, Repository, RepositoryInitOptions};
use std::path::Path;

/// Construct Git repository fixture.
pub struct GitFixture {
    repo: Repository,
}

impl GitFixture {
    /// Initialize new Git repository fixture.
    ///
    /// # Errors
    ///
    pub fn new(path: impl AsRef<Path>, kind: GitKind) -> Result<Self> {
        let mut opts = RepositoryInitOptions::new();
        opts.initial_head("main");
        opts.bare(kind.is_bare());
        let repo = Repository::init_opts(path.as_ref(), &opts)?;

        let mut config = repo.config()?;
        config.set_str("user.name", "John Doe")?;
        config.set_str("user.email", "john@doe.com")?;

        if kind == GitKind::Bare {
            config.set_str("status.showUntrackedFiles", "no")?;
            config.set_str("core.sparseCheckout", "true")?;
        }

        Ok(Self { repo })
    }

    /// Stage and commit new file into repository fixture.
    ///
    /// Directly stages file data into the tree as a blob, and commits the changes made. Generally,
    /// this method tries to avoid creating files and directories for a given file by directly
    /// editing the repository's tree data.
    ///
    /// # Errors
    ///
    pub fn stage_and_commit(
        &self,
        filename: impl AsRef<Path>,
        contents: impl AsRef<str>,
    ) -> Result<()> {
        let entry = IndexEntry {
            ctime: IndexTime::new(0, 0),
            mtime: IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o100644,
            uid: 0,
            gid: 0,
            file_size: contents.as_ref().len() as u32,
            id: self.repo.blob(contents.as_ref().as_bytes())?,
            flags: 0,
            flags_extended: 0,
            path: filename.as_ref().as_os_str().to_string_lossy().into_owned().as_bytes().to_vec(),
        };

        let mut index = self.repo.index()?;
        index.add_frombuffer(&entry, contents.as_ref().as_bytes())?;
        let tree_oid = index.write_tree()?;
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
            format!("Add {:?}", filename.as_ref()).as_ref(),
            &tree,
            &parents,
        )?;

        Ok(())
    }
}

/// Git fixture variants.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum GitKind {
    /// Normal Git repository fixture.
    #[default]
    Normal,

    /// Bare Git repository fixture.
    Bare,
}

impl GitKind {
    pub(crate) fn is_bare(&self) -> bool {
        match self {
            GitKind::Normal => false,
            GitKind::Bare => true,
        }
    }
}
