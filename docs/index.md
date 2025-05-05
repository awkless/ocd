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

<a href="usage">
  <button style="left-margin: 50%; right-margin: 50%; font-size: 20px;">
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

{% for post in site.posts %}
  <h3><a href="{{ post.url }}">{{ post.title }}</a></h3>
  <small>
    <strong>
      {{ post.date | date: "%B %e, %Y" }}
    </strong>
  </small>
{% endfor %}

## Acknowledgements

- Arch Linux Wiki page about [dotfiles][archwiki-dotfiles], which provided a
  great introduction about using Git to manage dotfiles using the bare-alias
  technique.
- Richard Hartmann's [vcsh][vcsh-git] and [myrepos][mr-git] tools, which
  generally provided the overall look and feel of OCD's command set.

[archwiki-dotfiles]: https://wiki.archlinux.org/title/Dotfiles
[vcsh-git]: https://github.com/RichiH/vcsh
[mr-git]: https://github.com/RichiH/myrepos
