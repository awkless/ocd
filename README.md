<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT or Apache-2.0
-->

# OCD

Organize current dotfiles.

This tool provides a way to manage dotfiles using a cluster of _normal_ and
[bare-alias][archwiki-dotfiles] Git repositories. A _bare-alias_ repository is a
special type of Git repository that allows a target directory to be used as an
alias for a worktree. This enables the target directory to be treated like a Git
repository without the need for initialization.

This approach facilitates the organization of configuration files across
multiple Git repositories without the necessity of copying, moving, or creating
symlinks for the files. It allows for a more modular method of managing
dotfiles.

## Installation

> __TODO__: Need to publish OCD to Crates.io.

## Usage

> __TODO__: Need to further hash out the details of OCD's CLI.

## Contribution

The OCD coding project is open to contribution.

See the [contribution guidelines][contrib-guide] for more information about
contributing to the project.

## License

The OCD project abides by the MIT and Apache-2.0 licenses for distribution of
its source code and documentation. The project also uses the CC0-1.0 license to
place files in the public domain, which are considered to be to small, or to
generic to place copyright over.

The project uses the [REUSE 3.3 specification][reuse-3.3] to make it easier to
determine who owns the copyright and licensing of any given file in the
codebase. The [Developer Certificate of Origin version 1.1][linux-dco] is also
used to ensure that any contributions made have the right to be merged into the
project, and can be distributed with the project under its main licenses.

[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles#Tracking_dotfiles_directly_with_Git
[contrib-guide]: ./CONTRIBUTING.md
[linux-dco]: https://developercertificate.org/
[reuse-3.3]: https://reuse.software/spec-3.3/
