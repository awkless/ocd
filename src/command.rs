// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::{
    fs::read_to_config,
    model::Cluster,
    path::{config_dir, data_dir},
    store::{MultiNodeClone, Root},
    Result,
};

use clap::{Parser, Subcommand};
use std::fs::remove_dir_all;

#[derive(Debug, Clone, Parser)]
#[command(
    about,
    override_usage = "\n  ocd [options] <ocd-command>\n  ocd [options] [pattern]... <git-command>",
    subcommand_help_heading = "Commands",
    version
)]
pub struct Ocd {
    /// Command-set interfaces.
    #[command(subcommand)]
    pub command: Command,
}

impl Ocd {
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            Command::Clone(opts) => run_clone(opts).await,
        }
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Clone existing cluster.
    #[command(override_usage = "ocd clone [options] <url>")]
    Clone(CloneOptions),
}

/// Clone existing cluster.
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct CloneOptions {
    /// URL to root repository to clone from.
    #[arg(value_name = "url")]
    pub url: String,

    /// Number of threads to use per node clone.
    #[arg(short, long, value_name = "limit")]
    pub jobs: Option<usize>,
}

async fn run_clone(opts: &CloneOptions) -> Result<()> {
    let _ = match Root::new_clone(&opts.url) {
        Ok(root) => root,
        Err(error) => {
            remove_dir_all(data_dir()?)?;
            return Err(error);
        }
    };

    let cluster = match read_to_config::<Cluster>(config_dir()?.join("cluster.toml")) {
        Ok(cluster) => cluster,
        Err(error) => {
            remove_dir_all(config_dir()?)?;
            return Err(error);
        }
    };

    let multi_clone = MultiNodeClone::new(&cluster, opts.jobs)?;
    multi_clone.clone_all().await?;

    Ok(())
}
