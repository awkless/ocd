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
//! [archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles#Tracking_dotfiles_directly_with_Git

#![allow(dead_code, clippy::missing_docs_in_private_items)]
#![warn(missing_docs, clippy::missing_errors_doc, clippy::missing_panic_doc)]

pub mod cluster;
pub mod utils;
pub mod vcs;

#[cfg(test)]
mod tests;
