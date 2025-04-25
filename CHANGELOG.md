<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

# Changelog

## [0.5.0] - 2025-04-25

### Added

- Implement OCD remove command.
- Add `Root::nuke` to nuke cluster through root repository.

## [0.4.0] - 2025-04-24

### Added

- Implement OCD init command.
- Add `crate::fs::write_to_config` to write serialized configuration data to
  external file.
- Add `Node::new_init` to initialize new node in repository store.
- Add `Root::new_init` to initialize new root in repository store.

## [0.3.0] - 2025-04-23

### Added

- Implement OCD undeploy command.
    - Simply the opposite of deploy command.

## [0.2.0] - 2025-04-23

### Added

- Implement OCD deploy command.
- Add deployment methods to `crate::store::{Root, Node}` types.
- Add `Cluster::get_node` to get node entries from cluster definition.
- Add `Node::new_open` to open existing node in repository store.
- Add `Root::new_open` to open existing root in repository store.
- Add `GitBuilder::open` to allow for opening existing repositories.

## [0.1.0] - 2025-04-23

### Added

- Implement OCD clone command.
    - Setup basic CLI.
- Add `crate::fs::read_to_config` to read and deserialize configuration files.
- Add `crate::store::MultiNodeClone` to perform the cloning of all nodes in
  cluster definition asynchronously.
- Add `crate::store::Root::new_clone` to clone and deploy root repository.
- Add `crate::utils::glob_match` to perform unix-like pattern matching on a set
  of strings.
- Add `crate::path::data_dir` to locate and create path to data directory.
- Add `crate::utils::syscall_interactive` to perform an interactive system call
  to external program.
- Add `crate::utils::syscall_non_interactive` to perform non-interactive system
  call to external program in order to obtain piped output of the call.
- Add `crate::model::Cluster::remove_node` to remove node entries from cluster
  definition.
- Add `crate::model::Cluster::add_node` to add new node entries into cluster
  definition.
- Add `crate::model::Cluster::dependency_iter` to obtain a valid iterable path
  through the dependencies of a target node entry.
- Add `crate::model::Cluster::from_str` to parse and deserialize string data
  into a valid `Cluster` type.
    - Add check to ensure that all node dependencies are defined in cluster.
    - Add check to ensure that all node dependencies are acyclic.
    - Add check to expand all environment variables used as directory aliases.
- Add `crate::exit_status_from_error` to obtain a valid exit status code from
  `anyhow::Error`.
- Add code quality checks as a CI workflow.
    - Check code style.
    - Run all tests and ensure they pass.
    - Perform static analysis on codebase.
    - Make sure all code is REUSE 3.3 compliant.
    - Make sure all dependencies are compatible with project license.
- Define `main` in `src/bin/ocd.rs`.
- Add `deny.toml` file to use `cargo-deny` tool on project dependencies to
  verify they are compatible with project license.
- Add feature request template.
- Add bug report template.
- Add pull request template.
- Make @awkless main codeower of project.
- Add `.gitattributes` file to define default textual attributes in project.
- Add `clippy.toml` to configure `clippy` linting.
- Add `rustfmt.toml` to configure `rustfmt` tool.
- Add `REUSE.toml` to state license information for `Cargo.lock`.
- Add `Cargo.toml` to define dependencies and project packaging details.
- Add `CODE_OF_CONDUCT.md` file to provide basic code of conduct.
- Add `CONTRIBUTING.md` file to provide basic contribution guidelines.
- Add `README.md` file to introduce newcomers to project.
- Add `.gitignore` file to ignore `target/*` directory.
- Add CC0-1.0 license.
- Add MIT license.

[0.4.0]: https://github.com/awkless/ocd/tag/v0.4.0
[0.3.0]: https://github.com/awkless/ocd/tag/v0.3.0
[0.2.0]: https://github.com/awkless/ocd/tag/v0.2.0
[0.1.0]: https://github.com/awkless/ocd/tag/v0.1.0
