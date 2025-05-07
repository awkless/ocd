---
layout: default
title: OCD Usage Guide
---

<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

## Getting Started

OCD stands for _organize current dotfiles_. It is a dotfile management tool that
operates on the concept of a _cluster_. This page will show you how to use it!

## Installation

Make sure you have the following pieces of software already installed _before_
attempting to install OCD itself:

- [Git][git-scm] [>= 2.30.0]
- [Rust][rust-lang] [>= 2021 Edition]

Through Cargo simply type the following into your terminal:

```
cargo install ocd --locked
```

Currently, there are no packaged version of OCD for major Linux distributions,
but hopefully that will change as OCD becomes more mature. Finally, you can also
directly build the project yourself by cloning and using Cargo.

## What is a "cluster"?

A cluster is a collection of repositories that can be deployed together. It is
comprised of two basic repository entries: __root__ and __node__.  The node
entry type can either be __normal__ or __bare-alias__. A _normal_ repository
entry is just a regular Git repository whose _gitdir_ and _worktree_ point to
the same path. A [bare-alias][archwiki-dotfiles] repository entry is a bare Git
repository that uses an external directory as an alias for a worktree.

The _root_ of a cluster is a special bare-alias repository that contains the
cluster definition itself, i.e. the special `cluster.toml` file that describes
all entries apart of the user's cluster. There can only exist one root, and it
is always deployed such that it cannot be undeployed. Removal of the root
repository will cause the entire cluster to be removed at once.

The _nodes_ of a cluster are the various repositories containing dotfile
configurations. A given node can either be defined as normal or bare-alias. Any
node can be added, removed, deployed, or undeployed from a cluster at any time
through OCD's command-set.

## Quick Start

First, construct a new root repository through the `init` command:

```
ocd init --root
```

Lets assume you want to configure the Vim text editor. Initialize a new node
repository named "vim" whose alias directory points to your home directory like
so:

```
ocd init --home-alias vim
```

A new node repository will now be initialized in your repository store, and a
basic template entry is provided in your cluster definition at
`$XDG_CONFIG_HOME/ocd/cluster.toml`. The cluster definition should look similar
to this when you open it up:

```
[nodes.vim]
deployment = "bare_alias"
url = ""
```

Assume you have a remote repository at "https://github.com/user/vim.git", go
ahead and fill the `url` field in for the "vim" node entry. It is important to
provide a URL for each node entry, so OCD always knows where to clone the
repository when it needs to. You do not need to provide a URL right away, but
you should do so as soon as possible. So your cluster definition should now look
like this:

```
[nodes.vim]
deployment = "bare_alias"
url = "https://github.com/user/vim.git"
```

> __NOTE__
> The `deployment` field uses syntactic sugar to specify that the "vim" node
> uses your home directory as its directory alias. When de-sugared, it will look
> like this:
>
> ```
> [nodes.vim]
> deployment = { kind = "bare_alias", dir_alias = "$HOME" }
> url = "https://github.com/user/vim.git"
> ```

Now you can write the `.vimrc` file in your home directory. Then, you can stage,
commit, and push the `.vimrc` file. You can perform Git operations directly
through OCD by issuing the name of the node you want to operate on, and the Git
command you want to use it like so:

```
ocd vim add .vimrc
ocd vim commit -m "Initial commit"
ocd vim push -u origin main
```

> __WARNING__
> Make sure you set up your remote for the "vim" node in order for the push
> operation to work:
>
> ```
> ocd vim remote add origin "https://github.com/user/vim.git"
> ```

That is it! You can continue to modify your "vim" node, or add new nodes by
either using the `init` command, or modifying your cluster definition file.
Continue reading the next sections for more advanced usage!

[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles
