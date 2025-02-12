<!--
SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
SPDX-License-Identifier: MIT
-->

# Contributing

Thanks for taking the time to contribute!

> __NOTE__: Remember that the information stored in this document only provides
> basic _guidelines_. Thus, all contributors are expected to use their bast
> judgement!

## Where to Submit Stuff

Send patches, questions, and discussions for the OCD project to Awklesses'
public inbox mailing list at `~awkless/public-inbox@lists.sr.ht`. Be sure to
verify that the whatever you send is not a duplicate in the mailing list, which
you can visit in a web browser [right here][public-inbox].

When posting patches to the mailing list, please make sure to edit the patch
line to include the name of the project, which is `ocd`. For example:

`[PATCH ocd v2] Add this and that`

> __NOTE__: Patches will only be considered if they pass the code quality check
> CI workflow.

## Coding Style

The OCD project uses the [Rust][rust-lang] programming langauge. Rust already
comes with a general [style and coding standard][rust-style] that should be
followed. To make development easier, use the `rustfmt` tool to automaically
format any piece of code, use `clippy` to lint your code, and use `cargo test`
to activate unit and integration testing to see if your code does not break
anything in the codebase.

## Commit and Patch Style

Generally follow these [guidelines][patch-guide] for commit and patch
submissions to Awklesses' public inbox mailing list. Here is another
[article][commit-ref] that provides a basic overview of proper commit
formatting.

The OCD project uses the [Developer Certificate of Origin version
1.1][linux-dco]. All commits need to have the following trailer:

```
Signed-off-by: <name> <email>
```

> __NOTE__: Make sure that your commit history within a given patch is linear
> and rebasable. This project prefers the rebase merge method of repository
> management.

## Rules of Licensing and Copyright

This project abides by the [REUSE 3.3 specification][reuse-3.3-spec]
specification to determine the licensing and copyright of files in the code
base. Thus, all files must have the proper SPDX copyright and licensing tags at
the top always. Contributors can Use the [reuse tool][reuse-tool] to determine
if their changes are REUSE 3.3 compliant.

OCD uses the MIT license as its main source code and documentation license.
OCD also uses the CC0-1.0 license to place files in the public domain that are
considered to be to small or generic to place copyright over. Thus, for almost
all contributions you will use the MIT license.

Do not forget to include the following SPDX copyright identifier at the top of
any file you create along with the SPDX license identifier:

```
SPDX-FileCopyrightText: <year> <name> <email>
SPDX-License-Identifier: MIT
```

[public-inbox]: https://lists.sr.ht/~awkless/public-inbox
[rust-lang]: https://doc.rust-lang.org
[rust-style]: https://doc.rust-lang.org/beta/style-guide/index.html
[patch-guide]: https://kernelnewbies.org/PatchPhilosophy
[commit-ref]: https://wiki.openstack.org/wiki/GitCommitMessages#Information_in_commit_messages
[cc1.0.0]: https://www.conventionalcommits.org/en/v1.0.0/
[linux-dco]: https://en.wikipedia.org/wiki/Developer_Certificate_of_Origin
[reuse-3.3-spec]: https://reuse.software/spec-3.3/
[reuse-tool]: https://reuse.software/tutorial/
