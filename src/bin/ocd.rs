// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use ocd::{exit_status_from_error, Ocd};

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    let layer = fmt::layer().compact().with_target(false).with_timer(false).without_time();
    let filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info")).unwrap();
    tracing_subscriber::registry().with(filter).with(layer).init();

    if let Err(error) = run().await {
        tracing::error!("{error:?}");
        std::process::exit(exit_status_from_error(error));
    }

    std::process::exit(exitcode::OK);
}

async fn run() -> Result<()> {
    let ocd = Ocd::parse();
    ocd.run().await.with_context(|| "Command failure")
}
