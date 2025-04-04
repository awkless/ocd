// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT


use crate::utils::glob_match;

use pretty_assertions::assert_eq;
use rstest::rstest;

#[rstest]
#[case::match_all(["*"], ["foo", "bar", "baz"], ["foo", "bar", "baz"])]
#[case::match_single_glob(["*sh"], ["sh", "bash", "yash", "vim"], ["sh", "bash", "yash"])]
#[case::match_no_glob(["vim", "foo"], ["foo", "dwm", "bar", "vim"], ["vim", "foo"])]
#[case::no_match(["foo", "bar"], ["vim", "dwm", "sh"], Vec::<String>::new())]
fn smoke_glob_match(
    #[case] patterns: impl IntoIterator<Item = impl Into<String>>,
    #[case] entries: impl IntoIterator<Item = impl Into<String>>,
    #[case] expect: impl IntoIterator<Item = impl Into<String>>,
) {
    let mut expect = expect.into_iter().map(Into::into).collect::<Vec<String>>();
    let mut result = glob_match(patterns, entries);
    expect.sort();
    result.sort();
    assert_eq!(result, expect);
}
