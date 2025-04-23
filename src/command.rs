// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Command set implementation.
//!
//! This module is the forward facing API of internal library. It is meant to be used in `main` of
//! the OCD binary. The entire OCD command set is implemented right there!.

use crate::{
    fs::read_to_config,
    model::Cluster,
    path::{config_dir, data_dir},
    store::{MultiNodeClone, Root},
    Result,
};

use clap::{Parser, Subcommand};
use std::fs::remove_dir_all;

/// OCD public command set CLI.
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
    /// Run OCD command based on given arguments.
    ///
    /// # Panics
    ///
    /// May panic if given command implementation also panics.
    ///
    /// # Errors
    ///
    /// Will fail if given command implementation fails.
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            Command::Clone(opts) => run_clone(opts).await,
        }
    }
}

/// Full command-set of OCD.
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
            // INVARIANT: Wipe out cluster if root cannot be cloned or deployed.
            remove_dir_all(data_dir()?)?;
            remove_dir_all(config_dir()?)?;
            return Err(error);
        }
    };

    let cluster = read_to_config::<Cluster>(config_dir()?.join("cluster.toml"))?;
    let multi_clone = MultiNodeClone::new(&cluster, opts.jobs)?;
    multi_clone.clone_all().await?;

    Ok(())
}
