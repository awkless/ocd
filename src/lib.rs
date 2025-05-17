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
#![warn(
    clippy::complexity,
    clippy::correctness,
    missing_debug_implementations,
    rust_2021_compatibility
)]
#![doc(issue_tracker_base_url = "https://github.com/awkless/ocd/issues")]

//pub(crate) mod cmd;
pub mod model;
pub mod store;

use tracing::{instrument, warn};

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
