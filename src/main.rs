// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

#![allow(dead_code)]

mod cluster;
mod fs;
mod vcs;

use crate::{
    fs::DirLayout,
    vcs::{MultiNodeClone, RootRepo},
};

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::{fs::remove_dir_all, process};

#[derive(Debug, Parser)]
#[command(
    about,
    override_usage = "\n  ocd [options] <ocd-command>\n  ocd [options] [node_names]... <git-command>",
    subcommand_help_heading = "Commands",
    version
)]
pub struct Cli {
    #[command(flatten)]
    pub verbosity: Verbosity<InfoLevel>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(override_usage = "ocd clone [options] <url>")]
    Clone(CloneOptions),
}

/// Clone existing cluster.
#[derive(Args, Debug)]
pub struct CloneOptions {
    /// URL to root repository to clone from.
    #[arg(value_name = "url")]
    pub url: String,

    /// Number of threads to use per node clone.
    #[arg(short, long, value_name = "limit")]
    pub jobs: Option<usize>,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .format_target(false)
        .format_timestamp(None)
        .filter_level(log::LevelFilter::max())
        .format_indent(Some(8))
        .init();

    let code = match run().await {
        Ok(code) => code,
        Err(error) => {
            log::error!("{error:?}");
            ExitCode::Failure
        }
    }
    .into();

    process::exit(code);
}

async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    log::set_max_level(cli.verbosity.log_level_filter());

    let dirs = DirLayout::new()?;
    match cli.command {
        Command::Clone(args) => {
            let root = match RootRepo::new_clone(args.url, &dirs) {
                Ok(root) => root,
                Err(err) => {
                    remove_dir_all(dirs.data())?;
                    return Err(err);
                }
            };
            let cluster = match root.get_cluster() {
                Ok(cluster) => cluster,
                Err(err) => {
                    remove_dir_all(dirs.data())?;
                    return Err(err);
                }
            };

            let multi_clone = MultiNodeClone::new(&cluster, &dirs);
            multi_clone.clone_all(args.jobs).await?;
            root.deploy()?;
        }
    }

    Ok(ExitCode::Success)
}

#[derive(Debug)]
pub enum ExitCode {
    Success,
    Failure,
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        match code {
            ExitCode::Success => 0,
            ExitCode::Failure => 1,
        }
    }
}
