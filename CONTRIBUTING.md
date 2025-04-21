<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

# Contributing

Thanks for taking the time to contribute!

> __NOTE__: Remember that the information stored in this document only provides
> basic _guidelines_. Thus, all contributors are expected to use their best
> judgement!

## Where to Submit Stuff

Submit patches via pull request. Submit bug reports and features requests
through the issue tracker at <https://github.com/awkless/ocd.git>. Follow the
provided templates, and ensure that any additions or modification to the
codebase passes the CI/CD workflows setup for code quality verification.

## Coding Style

The OCD project uses the [Rust][rust-lang] programming langauge. Rust already
comes with a general [style and coding standard][rust-style] that should be
followed. To make development easier, use the `rustfmt` tool to automaically
format any piece of code, use `clippy` to lint your code, and use `cargo test`
to activate unit and integration testing to see if your code does not break
anything in the codebase.

## Commit Style

Generally follow these [guidelines][commit-ref] for writing a proper commit.
As for formatting commits, the OCD project follows this basic format:

- Subject line starts with capitalization.
- Subject line does not have ending punctuation, e.g., no periods, question
  marks, exclaimation marks, etc.
- Subject line is limited to a maximum line width of 80 characters (50
  characters is prefered but not the hard limit).
- Subject line uses imparative mood.
- Body and subject line are separated by a blank line.
- Body is limited to a maximum line width of 80 characters.
- Body and trailers are separated by a blank line.

The OCD project uses the [Developer Certificate of Origin version
1.1][linux-dco]. All commits need to have the following trailer:

```
Signed-off-by: <name> <email>
```

Be sure that your commits are clear, because they may be used in the changelog
of the project!

> __NOTE__: Make sure that your commit history within a given patch is linear
> and rebasable. This project prefers the rebase merge method of repository
> management.

The following is a basic example of a good commit:

```
Configure Cargo for OCD project

Initial setup of the OCD project through Cargo. I will be adding
dependencies later on when the need arises. For now, Cargo should
understand how to package the OCD project to Crates.io when I inevitably
release it.

I also decided to submit `Cargo.lock` into Git, because current practice
seems to follow this convention[1]:

> When in doubt, check `Cargo.lock` into the version control system
> (e.g. Git).

[1]: https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html

Signed-off-by: Jason Pena <jasonpena@awkless.com>
```

## Rules of Licensing and Copyright

This project abides by the [REUSE 3.3 specification][reuse-3.3-spec]
specification to determine the licensing and copyright of files in the code
base. Thus, all files must have the proper SPDX copyright and licensing tags at
the top always. Contributors can Use the [reuse tool][reuse-tool] to determine
if their changes are REUSE 3.3 compliant.

OCD uses the MIT license as its main source code and documentation license. OCD
also uses the CC0-1.0 license to place files in the public domain that are
considered to be to small or generic to place copyright over. Thus, for almost
all contributions you will use the MIT license.

Do not forget to include the following SPDX copyright identifier at the top of
any file you create along with the SPDX license identifier:

```
SPDX-FileCopyrightText: <year> <name> <email>
SPDX-License-Identifier: MIT
```

[rust-lang]: https://doc.rust-lang.org
[conv-commit]: https://gist.github.com/qoomon/5dfcdf8eec66a051ecd85625518cfd13
[rust-style]: https://doc.rust-lang.org/beta/style-guide/index.html
[commit-ref]: https://wiki.openstack.org/wiki/GitCommitMessages#Information_in_commit_messages
[cc1.0.0]: https://www.conventionalcommits.org/en/v1.0.0/
[linux-dco]: https://en.wikipedia.org/wiki/Developer_Certificate_of_Origin
[reuse-3.3-spec]: https://reuse.software/spec-3.3/
[reuse-tool]: https://reuse.software/tutorial/
