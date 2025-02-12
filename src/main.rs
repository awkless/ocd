// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

#![allow(dead_code)]

mod cluster;

use anyhow::Result;

fn main() -> Result<()> {
    let mut log = colog::default_builder();
    log.filter(None, log::LevelFilter::Info);
    log.init();

    log::info!("Hello from ocd!");

    Ok(())
}
