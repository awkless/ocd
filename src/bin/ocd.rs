// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use ocd::exit_status_from_error;

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    let format = fmt::layer().pretty();
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();
    tracing_subscriber::registry()
        .with(filter)
        .with(format)
        .init();

    if let Err(error) = run().await {
        tracing::error!("{error:?}");
        std::process::exit(exit_status_from_error(error));
    }

    std::process::exit(exitcode::OK);
}

async fn run() -> Result<()> {
    todo!();
}
