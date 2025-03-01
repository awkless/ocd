<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

# Changelog

## [0.4.0] - 2025-02-28

### Added

- Add OCD clone command.
  - OCD clone command will clone root repository, extract node information from
    `cluster.toml` file, and clone all nodes from that information in cluster.
- Add `RootRepo` to manipulate root repository of cluster.
  - Add `RootRepo::new_clone` to clone new root repository by URL.
  - Add `RootRepo::get_cluster` to extract cluster definition inside root
    repository.
  - Add `RootRepo::nuke` to nuke existing root repository along with cluster
    itself.
  - Add `RootRepo::deploy` to deploy root repository to target worktree alias,
    excluding any files listed in the `Cluster::excludes` field.
- Add `MultiClone` to provide interactive cloning of multiple node repositories
  in cluster definition.
  - Add `MultiClone::new` to construct new `MultiClone` type by cluster
    definition.
  - Add `MultiClone::clone_all` to clone all node repositories in cluster.
- Add `SparseManip` type to make it easier to manipulate sparse checkout.
- Add `Git2AuthPrompt` to prompt user for credentials for any `git2` crate
  routines that need it.
- Add `GitWrapper` type to handle any and all Git command and repository
  manipulation.

### Changed

- Rename `vcs` module to `repo` module.
   - Improve Git manipulation logic through `GitWrapper` type.

### Removed

- Remove `syscall_git` function.
  - Now handled by `GitWrapper`.
- Remove `git_init`.
  - Will eventually be implemented in `GitWrapper`.
- Remove `git_clone`.
  - Now handled by `GitWrapper`.

## [0.3.0] - 2025-02-17

### Added

- Add `vcs` module to handle Git repositories.
    - Add `syscall_git` to make system calls to Git binary.
    - Add `git_init` to initialize new Git repository.
    - Add `git_clone` to clone Git repository.
    - Add `RepoKind` to define how a target repository should be treated.
        - Add `AliasDir`to define alias worktree to target directory.
- Add `git2-rs` crate to make it easier to manipulate Git repositories.
- Add `indicatif` crate to display visual progress bar when needed.
- Add `inquire` crate to create fancy prompts for CLI.
- Add `shellexpand` crate to perform shell expansion on data.
- Add `run_script` crate to easily run the contents of a script in subshell.

### Changed

- Moved all internal tests into `tests` module.

## [0.2.0] - 2025-02-12

### Added

- Add `cluster` module to handle cluster configurations.
    - Add `Cluster` type to deserialize cluster configurations.
    - Add `Node` type to deserialize repository entries in cluster.
- Add `Cluster::from_str` to parse and deserialize strings to `Cluster`.
- Add `Cluster::dependency_iter` to iterate through dependencies of a `Node`.
- Add `Cluster::cycle_check` to ensure that `Node` dependencies are acyclic.
- Add `Cluster::expand_worktree` to shellexpand worktree paths in `Node`s and
  root of cluster.

## [0.1.0] - 2025-02-11

### Added

- Place project under MIT license.
- Add CC0-1.0 license to place some stuff in public domain.
- Add `README.md` file to introduce newcomers to project.
- Add `CONTRIBUTING.md` file to provide basic contribution guidelines.
- Add `.build.yaml` to perform codebase quality checks using build.sr.ht.
- Ignore auxiliary build files from Cargo through `.gitignore`.
- Define default textual attributes through `.gitattributes`.
- Define code style settings through `.rustfmt.toml`.
- Define default linting settings through `.clippy.toml`.
- Setup development environment.
    - Add `Cargo.toml` to define project settings to Cargo.
    - Add `Cargo.lock` to make it easier to reproduce build environment.
    - Add `REUSE.toml` to place `Cargo.lock` under CC0-1.0 license.
    - Add `deny.toml` to define accepted and banned licenses for dependencies.
    - Define `main` to begin putting code into.

[0.4.0]: https://git.sr.ht/~awkless/ocd/refs/v0.4.0
[0.3.0]: https://git.sr.ht/~awkless/ocd/refs/v0.3.0
[0.2.0]: https://git.sr.ht/~awkless/ocd/refs/v0.2.0
[0.1.0]: https://git.sr.ht/~awkless/ocd/refs/v0.1.0
