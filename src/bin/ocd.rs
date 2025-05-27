// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use ocd::cmd::Ocd;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    let layer = fmt::layer().compact().with_target(false).with_timer(false).without_time();
    let filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info")).unwrap();
    tracing_subscriber::registry().with(filter).with(layer).init();

    if let Err(error) = run().await {
        tracing::error!("{error:?}");
        std::process::exit(1);
    }

    std::process::exit(0);
}

async fn run() -> Result<()> {
    Ocd::parse().run().await
}
