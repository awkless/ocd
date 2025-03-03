// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

#![allow(dead_code)]

mod config;
mod repo;

#[cfg(test)]
mod tests;

use crate::{
    config::{Cluster, read_config, Node, Layout},
    repo::{MultiClone, RootRepo, NodeRepo},
};

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::{fs::remove_dir_all, process};

#[derive(Debug, Parser)]
#[command(
    about,
    override_usage = "\n  ocd [options] <ocd-command>\n  ocd [options] <cluster_ref> <git-command>",
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

    #[command(override_usage = "ocd deploy [options] [node_names]...")]
    Deploy(DeployOptions),
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

#[derive(Args, Debug)]
pub struct DeployOptions {
    #[arg(value_parser, num_args = 1.., value_delimiter = ',')]
    node_names: Vec<String>,
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

    let layout = Layout::new()?;
    match cli.command {
        Command::Clone(args) => {
            let root = match RootRepo::new_clone(args.url, &layout) {
                Ok(root) => root,
                Err(err) => {
                    remove_dir_all(layout.config_dir())?;
                    remove_dir_all(layout.data_dir())?;
                    return Err(err);
                }
            };

            let cluster = root.get_cluster()?;
            let multi_clone = MultiClone::new(&cluster, &layout);

            root.deploy()?;
            multi_clone.clone_all(args.jobs).await?;
        }
        Command::Deploy(args) => {
            let cluster: Cluster = read_config("cluster.toml", &layout)?;
            for node_name in args.node_names {
                for (name, node) in cluster.dependency_iter(node_name) {
                    // TODO: Handle case where user wants to deploy root.
                    let repo = NodeRepo::from_node(name, node, &layout);
                    repo.deploy()?;
                }
            }
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
