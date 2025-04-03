// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT


use crate::utils::glob_match;

use pretty_assertions::assert_eq;

#[track_caller]
fn check_glob_match(
    patterns: impl IntoIterator<Item = impl Into<String>>,
    entries: impl IntoIterator<Item = impl Into<String>>,
    expect: impl IntoIterator<Item = impl Into<String>>,
) {
    let mut expect = expect.into_iter().map(Into::into).collect::<Vec<String>>();
    let mut result = glob_match(patterns, entries);
    expect.sort();
    result.sort();
    assert_eq!(result, expect);
}

#[test]
fn smoke_glob_match() {
    check_glob_match(["*"], ["foo", "bar", "baz"], ["foo", "bar", "baz"]);
    check_glob_match(["*sh"], ["sh", "bash", "yash", "vim"], ["sh", "bash", "yash"]);
    check_glob_match(["vim", "foo"], ["foo", "dwm", "bar", "vim"], ["vim", "foo"]);
    check_glob_match(["foo", "bar"], ["vim", "dwm", "sh"], Vec::<String>::new());
}
