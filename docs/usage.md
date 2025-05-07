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
>
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
>
> Make sure you set up your remote for the "vim" node in order for the push
> operation to work:
>
> ```
> ocd vim remote add origin "https://github.com/user/vim.git"
> ```

> __NOTE__
>
> You can also issue Git commands to your root repository by using the "root"
> keyword like so:
>
> ```
> ocd root remote add origin "https://github.com/user/dots-root.git"
> ocd root git add ~/.config/ocd/cluster.toml
> ocd root commit -m "Add vim node"
> ocd root push -u origin main
> ```
>
> It is recommended that you always stage and commit any changes you make to
> your `cluster.toml` file via "root" whenever you make modifications. You can
> check for status like this:
>
> ```
> ocd root status
> ```

If you already have an existing repository you want to add into your cluster,
then all you need to do is add it into your cluster definition. For example,
say you have a repository containing configurations for Bash shell at
"https://github.com/user/bash.git", and you want to have it along with your
Vim configuration. All you need to do is the following in your cluster
configuration file:

```
[nodes.vim]
deployment = { kind = "bare_alias", dir_alias = "$HOME" }
url = "https://github.com/user/vim.git"

# Add this!
[nodes.bash]
deployment = "bare_alias"
url = "https://github.com/user/bash.git"
```

Now use OCD's `deploy` command to both clone and deploy that repository to your
home directory in one shot:

```
ocd deploy bash
```

Finally, if you want to transfer your configuration to a new machine, then all
you need to do is clone the root repository through OCD's `clone` command like
so (assuming the root remote repository is at https://github.com/user/
dots-root.git):

```
ocd clone https://github.com/user/dots-root.git
```

This clone the root repository, and all other nodes in the cluster configuration
file apart of the root repository as well in one shot.

> __NOTE__
>
> OCD will fail if a given root repository does not define a cluster
> configuration file in either of these expected locations in the repository
> itself:
>
> 1. cluster.toml
> 2. .config/ocd/cluster.toml

That is it! You can continue to modify your "vim" or "bash" nodes using Git
commands through OCD's CLI, or add new nodes by either using the `init`
command, or modifying your cluster definition file and using the `deploy`
command. Use OCD's `help` command for more information about its command-set,
or continue reading the next sections for more advanced usage!

## Node Dependencies

Sometimes your configurations can get very large and complex. OCD focuses on
providing modularity for your dotfiles. Thus, it is encouraged to break up your
configurations into smaller pieces, i.e., smaller self-contained repositories,
and fit them together like lego. One major feature that enables you to do this
is the dependency system for node entries in your cluster
definition/configuration file.

As an example, lets assume you have configurations for both Dash and Bash shell
environments in separate remote repositories. You also use a custom PS1 prompt
provided by agkozak's [Polyglot Prompt][polyglot-ps1] project. The plan you are
going for is to have the Dash repository act as the base configuration for your
Bash repository. Here is an example setup of that plan:

```
-- dash/.profile --
# Setup PS1...
if [ -f "${XDG_DATA_HOME:-$HOME/.local/share}/polyglot.sh" ]; then
  # shellcheck source=/dev/null
  . "${XDG_DATA_HOME:-$HOME/.local/share}/polyglot.sh"
else
  echo "could not find polyglot.sh, using default PS1" 2>&1
  PS1="$(logname)@$(uname -n) $PWD/ \$ "
  export PS1
fi

-- bash/.bash_profile --
# Load in default profile...
if [ -f "$HOME/.profile" ]; then
  # shellcheck source=/dev/null
  . "$HOME/.profile"
else
  echo ".profile not found, environment maybe unstable" 2>&1
fi
```

In the above example configurations, you can see that the "dash" repository
sources Ployglot Prompt, and "bash" repository sources the `.profile` file that
is provided by the "dash" repository. It is clear that "bash" relies on "dash",
and "dash" relies on Ployglot Prompt existing in
`$XDG_DATA_HOME/share/polyglot.sh`.

The following cluster configuration example showcases how you would use the node
dependency system to link all of these repositories together:

```
[nodes.dash]
deployment = "bare_alias"
url = "https://github.com/user/dash.git"
dependencies = ["polyglot_ps1"]

[nodes.polyglot_ps1]
deployment = { kind = "bare_alias", dir_alias = "$HOME/.local/share" }
url = "https://github.com/agkozak/polyglot.git"

[nodes.bash]
deployment = "bare_alias"
url = "https://github.com/user/bash.git"
dependencies = ["dash"]
```

Now issue the `deploy` command on the "bash" node:

```
ocd deploy bash
```

OCD will deploy the "dash", "polyglot_ps1", and "bash" nodes in one shot,
without having to directly specify either "dash" or "polyglot_ps1" nodes. As
seen, the node dependency system streamlines the deployment of nodes, allowing
you to separate your configurations into self contained repositories, and
compose them together as you see fit.

> __WARNING__
>
> OCD requires the following preconditions from any cluster definition:
>
> 1. Node dependencies must by acyclic.
> 2. Node dependencies exist in cluster.
>
> OCD always checks for these preconditions each time you run it, and will fail
> if your cluster configuration file violates any of them.

> __NOTE__
>
> Your root repository cannot take part in having dependencies, because it
> already manages the cluster definition. This makes any nodes you define
> inherit dependencies of the cluster itself by default.

[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles
[git-scm]: https://git-scm.com/downloads
[polyglot-ps1]: https://github.com/agkozak/polyglot
[rust-lang]: https://www.rust-lang.org/tools/install
