<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

![Quality Check][quality-badge] ![Crates.io Version][crates-release]

# OCD

Organize current dotfiles.

This tool provides a way to manage the user's dotfiles through a __cluster__. A
_cluster_ is a group of repositories that can be deployed together. Upon
deployment, the user can issue Git commands interactively to manage their
dotfiles within a given repository apart of their cluster.

## Installation

Make sure you have the following pieces of software already installed _before_
attempting to install OCD itself:

- openssl [>= 0.10]
- [Rust][rust-lang] [>= 2021 Edition]
- [Git][git-scm] [>= 2.30.0]

## Arch Linux

Currently, OCD has been packaged to Arch Linux through the AUR. The following
showcases how to use an AUR manager to get latest version OCD.

```
paru -S ocd
```

If you want the latest changes made to the project's Git repository, then
do this instead:

```
paru -S ocd-git
```

## Cargo

Through Cargo simply type the following into your terminal:

```
cargo install ocd --locked
```

Do note that the man pages will not be included when using this method.

## Usage

See the provided man pages, or view the online [usage guide][usage-guide].

## Acknowledgements

- Arch Linux Wiki page about [dotfiles][archwiki-dotfiles], which provided a
  great introduction about using Git to manage dotfiles using the bare-alias
  technique.
- Richard Hartmann's [vcsh][vcsh-git] and [myrepos][mr-git] tools, which
  generally provided the overall look and feel of OCD's command set.

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

[quality-badge]: https://img.shields.io/github/actions/workflow/status/awkless/ocd/quality.yaml?style=for-the-badge
[crates-release]: https://img.shields.io/crates/v/ocd?style=for-the-badge&logo=rust&label=ocd
[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles#Tracking_dotfiles_directly_with_Git
[git-scm]: https://git-scm.com/downloads
[rust-lang]: https://www.rust-lang.org/tools/install
[usage-guide]: https://www.awkless.com/posts/ocd-0.8.0-usage-guide/
[contrib-guide]: ./CONTRIBUTING.md
[linux-dco]: https://developercertificate.org/
[reuse-3.3]: https://reuse.software/spec-3.3/
[vcsh-git]: https://github.com/RichiH/vcsh
[mr-git]: https://github.com/RichiH/myrepos
