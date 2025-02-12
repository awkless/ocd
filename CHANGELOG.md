<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

# Changelog

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

[0.1.0]: https://git.sr.ht/~awkless/ocd/refs/v0.1.0
