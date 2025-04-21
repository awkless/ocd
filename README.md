<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

# OCD

Organize current dotfiles.

This tool provides a way to manage the user's dotfiles through a __cluster__. A
_cluster_ is a group of repositories that can be deployed together. Upon
deployment, the user can issue Git commands interactively to manage their
dotfiles within a given repository apart of their cluster.

A cluster is comprised of two basic repository entries: __root__ and __node__.
Both of these entry types can either be __normal__ or __bare-alias__. A _normal_
repository entry is just a regular Git repository whose _gitdir_ and _worktree_
point to the same path. A [bare-alias][archwiki-dotfiles] repository entry is a
bare Git repository that uses an external directory as an alias for a worktree.

The _root_ of a cluster is a special bare-alias repository that contains the
cluster definition itself, i.e. the special `cluster.toml` file that describes
all entries apart of the user's cluster. There can only exist one root, and it
is always deployed such that it cannot be undeployed. Removal of the root
repository will cause the entire cluster to be removed at once.

The _nodes_ of a cluster are the various repositories containing dotfile
configurations. A given node can either be defined as normal or bare-alias. Any
node can be added, removed, deployed, or undeployed from a cluster at any time
through OCD's command-set.

## Installation

> __TODO__: Need to publish OCD to Crates.io.

## Usage

> __TODO__: Need to further hash out the details of OCD's CLI.

## Contribution

The OCD coding project is open to contribution.

See the [contribution guidelines][contrib-guide] for more information about
contributing to the project.

## License

The OCD project abides by the MIT license for distribution of its source code
and documentation. The project also uses the CC0-1.0 license to place files in
the public domain, which are considered to be to small, or to generic to place
copyright over.

The project uses the [REUSE 3.3 specification][reuse-3.3] to make it easier to
determine who owns the copyright and licensing of any given file in the
codebase. The [Developer Certificate of Origin version 1.1][linux-dco] is also
used to ensure that any contributions made have the right to be merged into the
project, and can be distributed with the project under its main licenses.

[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles#Tracking_dotfiles_directly_with_Git
[contrib-guide]: ./CONTRIBUTING.md
[linux-dco]: https://developercertificate.org/
[reuse-3.3]: https://reuse.software/spec-3.3/
