// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::utils::*;

use simple_test_case::test_case;

#[test_case(
    vec!["*sh".into(), "[f-g]oo".into(), "d?o".into()],
    vec!["sh".into(), "bash".into(), "foo".into(), "goo".into(), "doo".into()],
    vec!["sh".into(), "bash".into(), "foo".into(), "goo".into(), "doo".into()];
    "match all"
)]
#[test_case(
    vec!["foo".into(), "bar".into()],
    vec!["vim".into(), "dwm".into(), "sh".into()],
    Vec::<String>::new();
    "no match"
)]
#[test_case(
    vec!["[1-".into(), "[!a-d".into()],
    vec!["vim".into(), "dwm".into(), "sh".into()],
    Vec::<String>::new();
    "invalid pattern"
)]
#[test]
fn smoke_glob_match(patterns: Vec<String>, entries: Vec<String>, mut expect: Vec<String>) {
    let mut result = glob_match(patterns, entries);
    expect.sort();
    result.sort();
    pretty_assertions::assert_eq!(result, expect);
}

#[test_case(
    "git",
    vec!["ls-files".into(), "README.md".into()],
    Ok("stdout: README.md".into());
    "no error"
)]
#[test_case(
    "not_found",
    vec!["fail".into()],
    Err(anyhow::anyhow!("should fail"));
    "no program"
)]
#[test_case(
    "cd",
    vec!["--bad-flag".into()],
    Err(anyhow::anyhow!("should fail"));
    "invalid args"
)]
#[test]
fn smoke_syscall_non_interactive(
    cmd: &str,
    args: Vec<String>,
    expect: Result<String, anyhow::Error>,
) {
    let result = syscall_non_interactive(cmd, args);
    match expect {
        Ok(message) => assert_eq!(result.unwrap(), message),
        Err(_) => assert!(result.is_err()),
    }
}
