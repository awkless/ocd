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

## Usage

First things first, initialize a new root repository like so:

```
ocd init --root
```

The above will create a new root for your cluster. Root is a special bare-alias
repository that contains the cluster definition, i.e., special configuration
file located at `$XDG_CONFIG_HOME/ocd/cluster.toml`. This repository is always
deployed such that it cannot be undeployed. Without it OCD will not know how
to manage your cluster.

Lets assume that you want to configure dotfiles for Bash shell as an example.
Initialize a new repository named bash:

```
ocd init --home-alias bash
```

The command above will initialize a bare-alias repository named "bash" in your
repository store, and will set the default directory alias to your home
directory in your cluster definition. You should see the following in your
cluster definition:

```
[nodes.bash]
deployment = "bare_alias"
url = ""
```

The above is called a _node entry_, or _node_. A node provides various settings
that help OCD determine how to treat a given repository in your repository
store. For now you only have "root" and "bash" as entries in your repository
store.

Lets assume that you have a remote already setup to push changes. So fill in
the `url` field above with the URL of that remote repository:

```
[nodes.bash]
deployment = "bare_alias"
url = "https://github.com/user/bash.git"
```

Now assume that you already have written the following files to stage and commit
into the "bash" node in your home directory, and your current working directory
is your home directory:

- A `.bashrc` file.
- A `.bash_profile` file.
- A `README.md` file.

To add these files, simply issue Git commands through OCD by using the name of
the repository you want to issue those commands to like so:

```
ocd "bash" remote add origin https://github.com/user/bash.git
ocd "bash" add .bashrc .bash_profile README.md
ocd "bash" commit -m "Initial commit"
ocd "bash" push -u origin main
```

If you want to issue Git commands to multiple Git repositories you can use
unit-like matching patterns, and comma separated lists. For example:

```
ocd "*sh,foo,bar" remote add origin https://github.com/user/bash.git
ocd "b?sh" add .bashrc .bash_profile README.md
ocd "[a-b]?sh, ?oo, *ar" commit -m "Initial commit"
ocd "root,*" push -u origin main
```

If you want to issue Git commands to your root repository, then you need to
spell out root in full:

```
ocd "root" status
```

The separation of root from nodes of your cluster is mainly done to make it a
little harder to shoot yourself in the foot. It is recommended that you should
always use quotes around patterns you wish to use to match repositories to issue
commands upon.

Moving on, lets take advantage of the file exclusion feature that OCD offers
upon deployment of a given repository. You may not always want that `README.md`
file to always be deployed, so add this to your cluster definition:

```
[nodes.bash]
deployment = "bare_alias"
url = "https://github.com/user/bash.git"
excluded = ["README*"]
```

You can also use "README.md", but the `excluded` field accepts gitignore-style
patterns for excluding files from deployment. Now undeploy the excluded files of
"bash" like so:

```
ocd undeploy --excluded-only "bash"
```

Now only `.bashrc` and `.bash_profile` should be deployed, with `README.md`
undeployed. You can bring back `README.md` using the `deploy` command:

```
ocd deploy --with-excluded "bash"
```

Okay, now assume that you want to use an external repository to configure your
PS1 in bash. Lets use <https://github.com/agkozak/polyglot> for this. Go into
your cluster definition and add the following:

```
[nodes.bash]
deployment = "bare_alias"
url = "https://github.com/user/bash.git"
excluded = ["README*"]
dependencies = ["polyglot_prompt"]

[nodes.polyglot_prompt]
deployment = { kind = "bare_alias", dir_alias = "$HOME/.local/share"
url = "https://github.com/agkozak/polyglot.git"
excluded = [
  ".github/",
  "img/",
  ".gitignore",
  ".vimrc.local",
  "LICENSE",
  "README.md",
  "polyglot.plugin.zsh"
]
```

The above showcases an extended form of the `deployment` field for the
"polyglot\_prompt" node. This is how the field is supposed to be written in
full. In fact, the `deployment = "bare\_alias"` field in "bash" is just
syntactic sugar for:

```
deployment = { kind = "bare\_alias", dir\_alias = "$HOME" }
```

The `excluded` field for "polyglot\_prompt" essentially ignores most of its
worktree except for "polyglot.sh", which will be deployed to
`$HOME/.local/share"`.

Finally, we specify that "polyglot\_prompt" as dependency of "bash" through the
`dependencies` field. Now if we use the following command like so:

```
ocd deploy "bash"
```

The "bash" node will be deployed, along with "polyglot\_prompt". The deploy
command will automatically clone "polyglot\_prompt" into the repository store,
and deploy it.

Finally, assume that you have staged and committed all changes to your root
repository to a remote like so:

```
ocd "root" remote add origin https://github.com/user/dots-root.git"
ocd "root" add "$XDG_CONFIG_HOME/ocd/*"
ocd "root" commit -m "Initial commit"
ocd "root" push -u origin main
```

Now you want to transfer your cluster to a different machine. All you have to
do is use OCD's clone command:

```
ocd clone https://github.com/user/dots-root.git
```

This will clone your root repository and deploy it. Afterwards, this command
will clone all nodes of your repository asynchronously into your repository
store on the new machine.

Given that you only have access to one root at any given time per cluster. If
you want to use a new cluster configuration, when you already have an existing
cluster on your machine, you can simply use the `rm` command to nuke your
cluster like so:

```
ocd rm "root"
```

You will be prompted about really doing this. If you accept, all nodes
(including root) will be undeployed, and the configuration directory and
repository store will be removed in one-shot recursively.

You can just use the `rm` command to remove regular nodes from your cluster,
e.g., `ocd rm "bash"` or something similar. This will just remove the node from
your cluster configuration and repository store.

Finally, you can list all entries in your cluster with some fancy status
information using the `ls` command:

```
ocd ls
```

Use the `help` command, or `--help` flag to get more information about using
OCD, or a given OCD command.

Enjoy!

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
[git-scm]: https://git-scm.com/downloads
[rust-lang]: https://www.rust-lang.org/tools/install
[contrib-guide]: ./CONTRIBUTING.md
[linux-dco]: https://developercertificate.org/
[reuse-3.3]: https://reuse.software/spec-3.3/
