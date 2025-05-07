---
layout: default
---

<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

<h3 align="center">
  Organize current dotfiles!
</h3>

<a style="margin-left: 42%;" href="usage">
  <button>
    Get Started
  </button>
</a>

## Description

The OCD tool is a dotfile management tool that empowers the user to control the
structure and deployment of their configurations across multiple repositories
through the usage of a __cluster__. A _cluster_ is a group of repositories that
can be deployed together. Upon deployment of a repository, the user can issue
Git commands for further management directly through the OCD tool to keep track
of the changes they make to their dotfile configurations. Through their cluster,
the user can pick and choose which repositories get deployed on their machine,
offering finer control over their configuration across multiple machines.

## Dev Log

<ul>
{% for post in site.posts %}
<li>
<a href="{{ site.baseurl }}{{ post.url }}">{{ post.title }}</a><br/>
<small><strong>{{ post.date | date: "%B %e %Y" }}</strong></small>
</li>
{% endfor %}
</ul>

## Acknowledgements

- Arch Linux Wiki page about [dotfiles][archwiki-dotfiles], which provided a
  great introduction about using Git to manage dotfiles using the bare-alias
  technique.
- Richard Hartmann's [vcsh][vcsh-git] and [myrepos][mr-git] tools, which
  generally provided the overall look and feel of OCD's command set.

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

[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles
[vcsh-git]: https://github.com/RichiH/vcsh
[mr-git]: https://github.com/RichiH/myrepos
[linux-dco]: https://developercertificate.org/
[reuse-3.3]: https://reuse.software/spec-3.3/
