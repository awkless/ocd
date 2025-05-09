<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

# Changelog

## [0.7.0] - 2025-05-08

### Added

- Add hook command system!
  - Add `crate::model::HookRunner` to parse and execute hooks for a given
    target command in OCD.
  - Add `crate::model::CommandHooks` to perform parsing and deserialization of
    hook configuration file for `crates::model::HookRunner`.
  - Integrate `crate::model::HookRunner` into command set defined in `cmd`
    module.
- Create a GitHub page for OCD project.
- Digitize dev logs on new GitHub page for project.
- Add badges for current CI workflow status, and crates version to `README.md`
- Add "Acknowledgements` section to `README.md`

### Changed

- Move usage guide from `README.md` into usage section of the OCD GitHub page.

## [0.6.2] - 2025-05-05

### Added

- Add `crate::store::RepoEntry` to manage common functionality needed to create
  and maintain repository entries in repository store.
- Add `crate::store::Deployment` to make it easier to implement deployment
  strategies for various entries in repository store.
- Add `crate::store::NormalDeployment` to manage deployment for normal
  repository entries.
- Add `crate::store::BareAliasDeployment` to manage deployment for bare-alias
  repository entries.
- Add `crate::store::RootDeployment` to manage deployment for root repository
  entry.
- Add test cases for public facing code in `crate::store` module using txtar
  format to construct repository fixtures to operate with.

### Changed

- Massively refactor `crate::store::Git` into smaller more self contained pieces
  of code. This was done to prevent `crate::store::Git` from becoming a
  __god class__.
- User is now limited to only deploying their root repository to two locations:
  OCD's configuration directory (default) or their home directory. See __fixed__
  section for why.

### Fixed

- Allowing user to deploy their root anywhere they want in their home directory
  was a mistake. This would allow the user to define a cluster that violates the
  expected structure OCD needs in order to operate on that cluster, e.g.,
  deployment to `$HOME/.local/share/ocd` (repository store) was possible and
  made no sense.  Thus, to fix this issue, and to make it easier to locate the
  cluster definition, the user is now limited to only two locations for root
  repository deployment: OCD's expected configuration directory, or their home
  directory.
- The `deploy` and `undeploy` commands now clone node repositories if they do
  not already exist in repository store.
- Fixed repository deployment check `crate::store::is_deployed` by finally
  correctly traversing the tree structure of a given repository entry. Before,
  this check would only iterate the top-level tree, meaning that top-level files
  in a given repository would only be matched. This would cause the `deploy`
  or `undeploy` commands to not properly detect when a given repository was
  deployed, and made matching nested excluded files essentially impossible,
  e.g., a sparsity rule like "dir1/dir2/dir3/\*" would have never matched.

## [0.6.1] - 2025-04-25

### Added

- Add `crate::store::Node::new_clone` to clone node repository from node entry.

### Changed

- Add installation instructions to `README.md` file.
- Add usage examples and instructions to `README.md` file.

### Fixed

- Deploy command will now try to clone missing node repositories instead of
  failing to open missing node repositories.
- Undeploy command will now try to clone missing node repositories instead of
  failing to open missing node repositories.

## [0.6.0] - 2025-04-25

### Added

- Add Git shortcut to OCD CLI to call Git binary on target repository in
  repository store interactively.
- Add `Git::is_empty` as a better way to identify an empty repository.

### Fixed

- Fix `crate::command::run_remove`, i.e., remove command, by targeting
  "cluster.toml" instead of "cluster", and actually writing changes to
  configuration file through `crate::fs::write_to_config`.
- The `crate::store::Git::is_deployed` method now uses `Git::is_empty` instead
  of using the repository's index.
    - Fixes bug of empty repositories never being detected.
- The `crate::store::Git::is_deployed` method now revwalks through tree pointed
  to by HEAD in order to obtain valid filenames in the repository instead of
  iterating through the repository's index.
    - This fixes the bug of exclude patterns not matching files in repository,
      and files not being properly deployed to directory alias.

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

[0.6.2]: https://github.com/awkless/ocd/tag/v0.6.2
[0.6.1]: https://github.com/awkless/ocd/tag/v0.6.1
[0.6.0]: https://github.com/awkless/ocd/tag/v0.6.0
[0.5.0]: https://github.com/awkless/ocd/tag/v0.5.0
[0.4.0]: https://github.com/awkless/ocd/tag/v0.4.0
[0.3.0]: https://github.com/awkless/ocd/tag/v0.3.0
[0.2.0]: https://github.com/awkless/ocd/tag/v0.2.0
[0.1.0]: https://github.com/awkless/ocd/tag/v0.1.0
