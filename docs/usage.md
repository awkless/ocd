---
layout: default
title: OCD Usage Guide
---

<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

{:toc}

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

> __WARNING__
>
> Version 0.7.0 introduces a bug where the init command will fail if the cluster
> definition is not already created. Thus, when creating a new root, perform the
> following beforehand:
>
> ```
> touch ~/.config/ocd/cluster.toml
> ```
>
> Version 0.8.0 will not only include a few new commands, but will also fix this
> issue. Sorry for the inconvenience. Stabilization efforts are still in
> progress at this moment.

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

## Multi Targeting

You may want to issue the same command to multiple nodes in your cluster. OCD
offers a way to do this by simply providing a comma-separated list of names:

```
ocd vim,sh,tmux pull origin main
ocd deploy vim,sh,tmux
ocd undeploy vim,sh,tmux
ocd rm vim,sh,tmux
```

Even cooler, you can use unix-style matching patterns along with comma-separated
lists to operate on multiple target nodes:

```
ocd "*" pull origin main
ocd deploy "[V-v]im, t?ux"
ocd undeploy "*sh*"
ocd rm "?im, [S-s]h, t*ux"
```

> __NOTE__
>
> Make sure you always quote your targets whenever you decide to use unix-style
> matching patterns. If you do not, then your shell may expand the patterns
> instead of OCD, resulting in some very weird behavior.

Finally, if you wish to operate on root, then you __must__ type out "root" in a
given target listing. Unix-style matching patterns do not apply to root. Thus,

```
ocd "*" status
```

Will only get the status of nodes, excluding root by default. If you wanted to
get status for _all_ entries in the cluster, then do the following:

```
ocd "root,*" status
```

This must always be done for any command that allows multi-targeting. Having
root be separate was done to make it harder to shoot yourself in the foot.
Especially, with the `rm` command, because if you type:

```
ocd rm *
```

This will only remove node entries, leaving root intact. However, the following:

```
ocd rm root
```

will cause OCD to prompt you about removing root. If you accept, then OCD will
nuke your entire cluster by undeploying all nodes, deleting the configuration
directory, and deleting the repository store in one shot.

## Node Dependencies

Sometimes your configurations can get very large and complex. OCD focuses on
providing modularity for your dotfiles. Thus, it is encouraged to break up your
configurations into smaller pieces, i.e., smaller self-contained repositories,
and fit them together like lego. One major feature that enables you to do this
is the dependency system for node entries in your cluster definition.

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

## File/Directory Exclusion

Bare-alias deployment has the unique caveat that the entire contents of a
repository gets deployed to the target directory alias. Sometimes, you may have
certain files that you generally do not want to have deployed to avoid
cluttering you home directory, e.g., readme files, license files, etc. OCD
offers a way to prevent this with its file/directory exclusion feature. Say you
have a node repository named "vim" with a readme that contains all the special
keybindings you have configured for it as a reference guide. You generally do
not want that readme file until you want to reference it, so you should do this
in your cluster definition:

```
[nodes.vim]
deployment = { kind = "bare_alias", dir_alias = "$HOME" }
url = "https://github.com/user/vim.git"
excluded = ["README*"]
```

Now when you use the `deploy` command, any file or directory that matches
"README*" for the "vim" node will be excluded from deployment. You can bring
back excluded files like so:

```
ocd deploy --with-excluded vim
```

You can also undeploy _only_ excluded files instead of the entire node like so:

```
ocd undeploy --only-excluded vim
```

Notice how the previous example showed the usage of a wildcard. You see, OCD
uses Git's sparse-checkout feature to perform the exclusion. Sparse-checkout
uses gitignore style rules for matching files and directories to include on
checkout. These same patterns can be used in the `excluded` field to prevent
certain files from being deployed. For example:

```
[nodes.ps1_polyglot]
deployment = { kind = "bare_alias", dir_alias = "$HOME/.local/share" }
url = "git@github.com:awkless/polyglot.git"
excluded = [
  ".github/",
  "img/",
  ".gitignore",
  ".vimrc?local",
  "**/LICENSE*",
  "README*",
  "[P-p]olyglot.plugin.zsh"
  "!polyglot.sh",
]
```

The root repository can also use the file/directory exclusion feature through a
top-level `excluded` field:

```
# Do not deploy these in root repository...
excluded = ["README*", "LICENSE*"]

# Node stuffs...
[nodes.vim]
deployment = { kind = "bare_alias", dir_alias = "$HOME" }
url = "https://github.com/user/vim.git"
excluded = ["README*"]
```

## Monolithic Structure

The previous sections of this text mainly showcased the modular structuring
features of OCD. While the tool encourages the user to employ a modular setup
for their dotfiles, monolithic structuring is still offered. Sometimes you just
want to plop stuff somewhere for safe keeping, and organize it later.

A monolithic cluster is created by only using the root repository, with no nodes
to store dotfiles. By default, OCD uses the following layout for root in your
cluster definition:

```
dir_alias = "config_dir"
```

In fact, this default is expected to be so common, that you do not even need to
directly specify it. This is how all previous examples shown to you operate, by
simply not stating the directory alias for root, causing OCD to simply use the
standard configuration directory at `$XDG_CONFIG_HOME/ocd`.

OCD allows you to change root's directory alias to a secondary location, which
is your home directory:

```
dir_alias = "home_dir"
```

By doing this, you must ensure that you place your cluster definition at
`.config/ocd/cluster.toml` within the root repository instead of the default
top-level location of just `cluster.toml` like you normally would for a modular
setup. Visual example:

```
# When using `dir_alias = "config_dir"`
|- root/
|-- cluster.toml

# When using `dir_alias = "home_dir"`
|- root/
|-- .config/
|--- ocd/
|---- cluster.toml
```

Now any dotfiles you want to have apart of your cluster can be directly
committed into root itself:

```
ocd root add .vimrc
ocd root commit -m "Add Vim config"
ocd root push origin main
```

No need to specify or define nodes! Now when you decide to clone your monolithic
cluster, it will just clone root and deploy everything in your home directory in
one shot. All operations for managing your cluster can now be done through just
the root repository for this setup.

Of course, OCD does not limit you to not having any nodes when having root use
your home directory as its directory alias. You can still do the following in
your cluster definition, and the command set will still work all the same:

```
dir_alias = "home_dir"
excluded = ["README*", "LICENSE*"]

[nodes.vim]
deployment = { kind = "bare_alias", dir_alias = "$HOME" }
url = "https://github.com/user/vim.git"
excluded = ["README*"]

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

Whether you decide to use your home directory or the OCD configuration directory
as a directory alias for root, you can mix and match functionality to best suit
your needs. OCD is all about flexibility!

> __NOTE__
>
> The "home_dir" and "config_dir" are the only two valid settings for root
> directory aliases. Nodes can generally be deployed anywhere relative to your
> home directory.

## Command Hooks

You can further enhance your configurations through the use of OCD's command
hook system. Hooks can be defined in the `$XDG_CONFIG_HOME/ocd/hooks.toml` file.
Each hook entry operates off of a hook script that is expected to be stored at
`$XDG_CONFIG_HOME/ocd/hooks/`.

For example, lets assume you have a repository that patches the
[st][suckless-st] (Suckless Terminal emulator) to your liking at
"https://github.com/user/st.git". You want it to built and installed each time
you run the `deploy` command. Here is how you can do that. First, create a hook
entry in your hook configuration file:

```
[hooks]
deploy = [
  { post = "make_install.sh", workdir = "$HOME/.local/share/ocd/st", node = "st" },
]
```

The above issues a hook script to be executed after the deploy command on the
target node named "st". This hook script will be executed in the "st" repository
housed in the repository store of your cluster. Next we make sure that the "st"
node is defined in our cluster definition:

> __NOTE__
>
> You can make hook scripts execute before a given command by using the `pre`
> field instead of the `post` field.

> __WARNING__
>
> When sourcing a hook script for either the `pre` or `post` fields for a given
> hook entry, __only__ give the name of the script, _not_ the absolute or
> relative path of the hook script. If you provide something like
> `pre = "/home/awkless/hooks/hook.sh"`, then OCD will try to look for
> `$XDG_CONFIG_HOME/ocd/hooks/home/awkless/hooks/hook.sh` instead.
>
> All hooks must always be defined in the `$XDG_CONFIG_HOME/ocd/hooks/`
> directory, because OCD will never look anywhere else. This is done to keep
> hooks in a single self contained area for easier review of their contents.
> Be suspicious of hook scripts provided by other people's clusters that source
> other scripts outside of the hooks directory!

```
[nodes.st]
deployment = "normal"
url = "https://github.com/user/st.git"
```

Moving on, we define the hook script in the special `$XDG_CONFIG_HOME/ocd/hooks`
directory:

```
#!/bin/sh

make clean
make
sudo make install
```

Finally, we execute the hook script by making a call to OCD's deploy command
on the "st" node:

```
ocd deploy st
```

By default, any hooks executed will be paged and prompted to you, asking if you
actually want to execute the hook. This is done to make you aware of what you
are making OCD do at all times as a security measure. To accept the hook hit the
"a" key, to deny the hook it the "d" key. The pager being used is provided from
the [minus][minus-repo] project.

If you already trust the hook or any other hook that may be executed for a given
command then you can use the "always" hook action to always execute hooks no
questions asked:

```
ocd --run-hook=always deploy st
```

If you do trust a given hook, or you just do not want any hooks to be executed
for whatever reason, then use the "never" hook action:

```
ocd --run-hook=never deploy st
```

The previous example showcased a _targeted hook_. Hooks can be written to not
target a specific node. Simply do not provide a `node` field when defining a
given hook:

```
[hooks]
deploy = [
  { post = "make_install.sh", workdir = "$HOME/.local/share/ocd/st" },
]
```

In the above hook configuration, this new hook will always execute no matter the
node specified to be deployed. So something like:

```
ocd deploy vim
```

Will stil cause the hook to be executed, whereas before the hook would have only
been executed if you directly specified the "st" node.

> __NOTE__
>
> Targeted hooks only operate on _nodes_. If you want to target root, then use
> a non-targeted hook.

Finally, not all commands offer support for hooks, or even targeted hooks. Here
is a helpful chart of current hook support for each command that OCD offers.

| Command      | Non-Targeted Hook Support | Targeted Hook Support |
| ------------ | ------------------------- | --------------------- |
| clone        | yes                       | no                    |
| init         | yes                       | no                    |
| deploy       | yes                       | yes                   |
| undeploy     | yes                       | yes                   |
| rm           | yes                       | yes                   |
| ls           | no                        | no                    |
| git shortcut | no                        | no                    |

Make sure that any hooks you define in both `$XDG_CONFIG_HOME/ocd/hooks.toml`
and `$XDG_CONFIG_HOME/ocd/hooks/` are committed into your root repository so you
can have them always deployed and ready to go when you transfer your cluster to
a new machine!

[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles
[git-scm]: https://git-scm.com/downloads
[polyglot-ps1]: https://github.com/agkozak/polyglot
[rust-lang]: https://www.rust-lang.org/tools/install
[suckless-st]: https://st.suckless.org/
[minus-repo]: https://github.com/AMythicDev/minus/
