// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! General utilities.
//!
//! Provides basic general miscellaneous utilities to make life easier. These utilities were placed
//! here either because they did not seem to fit the purpose of other modules, but were still
//! important enough to have around.

use crate::{Error, Result};

use std::{ffi::OsStr, process::Command};

/// Call external shell program non-interactively.
///
/// Will pipe stdout and stderr to child process, waiting to collect all output and combine it into
/// a singular string to be returned and handled by the caller. This child process cannot be
/// interacted with. In fact, any attempts to use stdin will close the stream.
///
/// The combined output of stdout and stderr is labeled "stdout: {stdout}" and "stderr: {stderr}"
/// in the returned string respectively. This is done to make it easy to extract either output
/// stream from the returned string for further processing once the external shell program is
/// finished executing.
///
/// # Errors
///
/// - Will fail if external shell program cannot be found.
/// - Will fail if given arguments for external shell program are invalid.
pub fn syscall_non_interactive(
    cmd: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<String> {
    let output = Command::new(cmd.as_ref()).args(args).output()?;
    let stdout = String::from_utf8_lossy(output.stdout.as_slice()).into_owned();
    let stderr = String::from_utf8_lossy(output.stderr.as_slice()).into_owned();
    let mut message = String::new();

    if !stdout.is_empty() {
        message.push_str(format!("stdout: {stdout}").as_str());
    }

    if !stderr.is_empty() {
        message.push_str(format!("stderr: {stderr}").as_str());
    }

    if !output.status.success() {
        return Err(Error::SyscallNonInteractive {
            program: cmd.as_ref().to_string_lossy().into_owned(),
            message,
        });
    }

    // INVARIANT: Chomp trailing newlines.
    let message = message
        .strip_suffix("\r\n")
        .or(message.strip_suffix('\n'))
        .map(ToString::to_string)
        .unwrap_or(message);

    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    use simple_test_case::test_case;

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
}
