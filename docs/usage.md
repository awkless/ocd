---
layout: default
---

<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

## Getting Started

OCD stands for _organize current dotfiles_. It is a dotfile management tool that
operates on the concept of a _cluster_.

A cluster is comprised of two basic repository entries: __root__ and __node__.
The node entry type can either be __normal__ or __bare-alias__. A _normal_
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

[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles
