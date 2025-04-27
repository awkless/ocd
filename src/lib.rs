// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Internal library for OCD tool.
//!
//! OCD stands for "Organize Current Dotfiles". It is a tool that provides a way to manage dotfiles
//! using a cluster of _normal_ and [bare-alias][archwiki-dotfiles] Git repositories. A
//! _bare-alias_ repository is a special type of Git repository that allows a target directory to
//! be used as an alias for a worktree. This enables the target directory to be treated like a Git
//! repository without the need for initialization.
//!
//! This approach facilitates the organization of configuration files across multiple Git
//! repositories without the necessity of copying, moving, or creating symlinks for the files. It
//! allows for a more modular method of managing dotfiles.
//!
//! See the `CONTRIBUTING.md` file about contribution to the OCD coding project if you have any
//! ideas or code to improve the project as a whole!
//!
//! ## The Concept of a Cluster
//!
//! A __cluster__ is a group of repositories that can be deployed together. It is comprised of two
//! major components: the __cluster definition__ and the __repository store__. The _cluster
//! definition_ defines all entries of a cluster within a special configuration file. The
//! _repository store_ houses all repositories defined as entries in the cluster definition.
//!
//! The cluster definition contains two entry types: __root__ and __node(s)__. A given _node_ entry
//! type can either be _normal_ or _bare-alias_. All node entries can be deployed, undeployed,
//! added, and removed at any time from the cluster definition. The _root_ is a special bare-alias
//! entry that hosues the cluster definition itself. There can only be __one__ root, and it must
//! _always_ be deployed such that it can never be undeployed. Removal of root will cause the
//! entire cluster to be removed along with it, included all defined node entries.
//!
//! The _repository store_ follows the same structure as the cluster definition. There exists a
//! root repository, and a set of node repositories within the repository store. The root
//! repository is simply named "root" at all times, and the node repositories are named whatever
//! name they were given in the cluster definition. The repository store always reflects the
//! changes made to the cluster definition such that a top-down heirarchy is followed, with the
//! cluster definition at the top and repository store at the bottom.
//!
//! [archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles#Tracking_dotfiles_directly_with_Git

#![allow(dead_code)]

pub(crate) mod command;
pub(crate) mod fs;
pub(crate) mod model;
pub(crate) mod store;
pub(crate) mod utils;

#[doc(hidden)]
pub use command::Ocd;

#[cfg(test)]
mod tests;

/// All possible error variants that OCD can encounter during runtime.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Cannot determine path to home directory")]
    NoWayHome,

    #[error("Cannot determine path to configuration directory")]
    NoWayConfig,

    #[error("Cannot determine path to data directory")]
    NoWayData,

    #[error(transparent)]
    Toml(#[from] toml_edit::TomlError),

    #[error("Dependency {name:?} not found in cluster definition")]
    DependencyNotFound { name: String },

    #[error("Cluster contains cycle(s): {cycle:?}")]
    CircularDependencies { cycle: Vec<String> },

    #[error("Expect {name:?} to be defined as a table")]
    EntryNotTable { name: String },

    #[error("Could not find {name:?} in cluster definition")]
    EntryNotFound { name: String },

    #[error(transparent)]
    Shellexpand(#[from] shellexpand::LookupError<std::env::VarError>),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Program {program:?} called non-interactively, but failed to execute\n{message}")]
    SyscallNonInteractive { program: String, message: String },

    #[error("Program {program:?} called interactively, but failed to execute")]
    SyscallInteractive { program: String },

    #[error(transparent)]
    Git2(#[from] git2::Error),

    #[error("Cannot find file {file:?} in index of {repo:?}")]
    Git2FileNotFound { repo: String, file: String },

    #[error("Cannot determine current branch of {repo:?}")]
    Git2UnknownBranch { repo: String },

    #[error(transparent)]
    ProgressStyle(#[from] indicatif::style::TemplateError),

    #[error(transparent)]
    Inquire(#[from] inquire::InquireError),

    #[error("Node not given a name")]
    NoNodeName,
}

impl From<Error> for i32 {
    fn from(error: Error) -> Self {
        match error {
            Error::NoWayHome => exitcode::IOERR,
            Error::NoWayConfig => exitcode::IOERR,
            Error::NoWayData => exitcode::IOERR,
            Error::Toml(..) => exitcode::CONFIG,
            Error::DependencyNotFound { .. } => exitcode::CONFIG,
            Error::CircularDependencies { .. } => exitcode::CONFIG,
            Error::EntryNotTable { .. } => exitcode::CONFIG,
            Error::EntryNotFound { .. } => exitcode::CONFIG,
            Error::Shellexpand(..) => exitcode::IOERR,
            Error::Io(..) => exitcode::IOERR,
            Error::SyscallNonInteractive { .. } => exitcode::OSERR,
            Error::SyscallInteractive { .. } => exitcode::OSERR,
            Error::Git2(..) => exitcode::SOFTWARE,
            Error::Git2FileNotFound { .. } => exitcode::SOFTWARE,
            Error::Git2UnknownBranch { .. } => exitcode::SOFTWARE,
            Error::ProgressStyle(..) => exitcode::SOFTWARE,
            Error::Inquire(..) => exitcode::SOFTWARE,
            Error::NoNodeName => exitcode::USAGE,
        }
    }
}

/// Wrapper to make it easy to specify [`Result`] using [`Error`].
pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// Obtain exit status from [`anyhow::Error`].
pub fn exit_status_from_error(error: anyhow::Error) -> i32 {
    match error.downcast::<Error>() {
        Ok(error) => error.into(),
        Err(_) => exitcode::SOFTWARE,
    }
}
