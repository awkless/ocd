// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

//! General utilities.
//!
//! This module provides general miscellaneous utilities to make life easier. These utilities where
//! placed here either because they did not seem to fit the purpose of other modules, but were
//! still important to have around.

use anyhow::{anyhow, Context, Result};
use mkdirp::mkdirp;
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

/// Determine absolute paths to required dirctories.
///
/// The OCD tool needs to know the absolute paths for the user's home directory, configuration
/// directory, and data directory. The home directory is often used as the default location when
/// the user does not define a worktree alias to use for node's and root of a cluster. The
/// configuration directory is where OCD will locate configuration file data that it needs to
/// operate with. This configuration directory is currently expected to be in
/// `$XDG_CONFIG_HOME/ocd`. Finally, the data directory is where all deployable node repositories
/// will be stored, which is currently expected to be in `$XDG_DATA_HOME/ocd`.
#[derive(Debug, Clone)]
pub struct DirLayout {
    home: PathBuf,
    config: PathBuf,
    data: PathBuf,
}

impl DirLayout {
    /// Construct new directory layout paths.
    ///
    /// Will construct paths to configuration and data directory if they do not already exist.
    ///
    /// # Errors
    ///
    /// - Will fail if home, configuration, or data directories cannot be determined for whatever
    ///   reason.
    /// - Will also fail if configuration or data directories cannot be constructed when needed.
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or(anyhow!("Cannot find home directory"))?;
        let config = dirs::config_dir().ok_or(anyhow!("Cannot find config directory"))?.join("ocd");
        let data = dirs::data_dir().ok_or(anyhow!("Cannot find data directory"))?.join("ocd");

        mkdirp(&config)?;
        mkdirp(&data)?;

        Ok(Self { home, config, data })
    }

    /// Path to home directory.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Path to configuration directory.
    pub fn config(&self) -> &Path {
        &self.config
    }

    /// Path to data directory.
    pub fn data(&self) -> &Path {
        &self.data
    }
}

/// Read and deserialize contents of target configuration file.
///
/// General helper that uses type annotations to determine the correct type to parse and deserialize
/// a given file target to. Will create an empty configuration file if it does not already exist.
///
/// # Errors
///
/// - Will fail if path cannot be opened, created, or read.
/// - Will fail if extracted string data cannot be parsed and deserialized into target type.
pub fn read_config<C>(path: impl AsRef<Path>, dirs: &DirLayout) -> Result<C>
where
    C: std::str::FromStr<Err = anyhow::Error>,
{
    let full_path = dirs.config().join(path.as_ref());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(false)
        .read(true)
        .create(true)
        .open(full_path)?;

    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;

    let config: C = buffer.parse()?;
    Ok(config)
}

/// Use Unix-like glob pattern matching.
///
/// Will match a set of patterns to a given set of entries. Whatever is matched is returned as a
/// new vector to operate with. Invalid patterns or patterns with no matches or excluded from the
/// new vector, and logged as errors.
///
/// # Invariants
///
/// - Always produce valid vector containing matched entries only.
/// - Process full pattern list without failing.
pub fn glob_match(
    patterns: impl IntoIterator<Item = impl Into<String>>,
    entries: impl IntoIterator<Item = impl Into<String>>,
) -> Vec<String> {
    let patterns = patterns.into_iter().map(Into::into).collect::<Vec<String>>();
    let entries = entries.into_iter().map(Into::into).collect::<Vec<String>>();

    let mut matched = Vec::new();
    for pattern in &patterns {
        let pattern = match glob::Pattern::new(pattern) {
            Ok(pattern) => pattern,
            Err(error) => {
                // INVARIANT: Error log invalid pattern, and skip over to next available pattern.
                log::error!("Invalid pattern {pattern:?}: {error}");
                continue;
            }
        };

        let mut found = false;
        for entry in &entries {
            // INVARIANT: Only include valid matches into vector.
            if pattern.matches(entry) {
                found = true;
                matched.push(entry.to_string());
            }
        }

        // INVARIANT: Error log unmatched pattern before moving to next available pattern.
        if !found {
            log::error!("Pattern {} does not match any entries", pattern.as_str());
        }
    }

    matched
}

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
    let args: Vec<OsString> = args.into_iter().map(|s| s.as_ref().to_os_string()).collect();
    let output = Command::new(cmd.as_ref())
        .args(args)
        .output()
        .with_context(|| format!("Failed to call {:?}", cmd.as_ref()))?;

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
        return Err(anyhow!("{:?} failed\n{message}", cmd.as_ref()));
    }

    Ok(message)
}

/// Call external shell program interactively.
///
/// Will inherit stdout and stderr from user's current working environment. Any output will be
/// issued to user interactively for their session.
///
/// Given that stdout and stderr are inherited, there is no need to collect output, because the
/// user will have already seen it. Thus, caller should use this method to allow user to interact
/// with a given shell program, and return control back the OCD program when finished.
///
/// # Errors
///
/// - Will fail if external shell program cannot be found.
/// - Will fail if given arguments for external shell program are invalid.
pub fn syscall_interactive(
    cmd: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<()> {
    let args: Vec<OsString> = args.into_iter().map(|s| s.as_ref().to_os_string()).collect();
    let status = Command::new(cmd.as_ref())
        .args(args)
        .spawn()
        .with_context(|| format!("Failed to call {:?}", cmd.as_ref()))?
        .wait()?;

    if !status.success() {
        return Err(anyhow!("Call to {:?} succeeded, but program failed to execute", cmd.as_ref()));
    }

    Ok(())
}
