// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

#![allow(dead_code)]

mod cluster;
mod vcs;

#[cfg(test)]
mod tests;

use anyhow::Result;

fn main() -> Result<()> {
    env_logger::Builder::new()
        .format_target(false)
        .format_timestamp(None)
        .filter_level(log::LevelFilter::max())
        .format_indent(Some(8))
        .init();

    log::info!("Hello from ocd!");

    Ok(())
}
