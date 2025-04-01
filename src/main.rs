// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

#![allow(dead_code)]

mod cluster;
mod utils;
mod vcs;

use crate::{
    cluster::Cluster,
    utils::{read_config, DirLayout, glob_match},
    vcs::{MultiNodeClone, NodeRepo, RootRepo},
};

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::{ffi::OsString, fs::remove_dir_all, process};

/// Command-line interface of OCD tool.
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

/// Full command set.
#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(override_usage = "ocd clone [options] <url>")]
    Clone(CloneOptions),

    #[command(override_usage = "ocd deploy [options] [node_names]...")]
    Deploy(DeployOptions),

    #[command(external_subcommand)]
    Git(Vec<OsString>),
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

/// Deploy node of cluster.
#[derive(Args, Debug)]
pub struct DeployOptions {
    /// List of nodes to deploy.
    #[arg(value_parser, num_args = 1.., value_delimiter = ',', value_name = "node_names")]
    pub node_names: Vec<String>,

    /// Do not deploy dependencies of target nodes.
    #[arg(short, long)]
    pub only: bool,

    /// Deploy excluded files as well.
    #[arg(short, long)]
    pub with_excluded: bool,
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
            let root = RootRepo::new_clone(args.url, &dirs).inspect_err(|_| {
                remove_dir_all(dirs.data()).ok();
            })?;
            let cluster = root.get_cluster().inspect_err(|_| {
                remove_dir_all(dirs.data()).ok();
            })?;

            let multi_clone = MultiNodeClone::new(&cluster, &dirs);
            multi_clone.clone_all(args.jobs).await?;
            root.deploy()?;
        }
        Command::Deploy(mut args) => {
            let (root, cluster) = if dirs.config().join("cluster.toml").exists() {
                let cluster: Cluster = read_config("cluster.toml", &dirs)?;
                let root = RootRepo::from_cluster(&cluster, &dirs);
                (root, cluster)
            } else {
                let root = RootRepo::new_open(&dirs)?;
                let cluster = root.get_cluster()?;
                root.deploy()?;
                (root, cluster)
            };

            args.node_names.dedup();
            for node_name in &mut args.node_names {
                node_name.retain(|c| !c.is_whitespace());
            }

            if let Some(index) = args.node_names.iter().position(|x| *x == "root") {
                args.node_names.swap_remove(index);
                if args.with_excluded {
                    root.deploy_all()?;
                } else {
                    root.deploy()?;
                }
            }

            let targets = glob_match(args.node_names, cluster.nodes.keys())?;
            for target in &targets {
                if args.only {
                    let node = cluster.get_node(target)?;
                    let repo = NodeRepo::new(target, node, &dirs);
                    if args.with_excluded {
                        repo.deploy_all()?;
                    } else {
                        repo.deploy()?;
                    }
                } else {
                    for (name, node) in cluster.dependency_iter(target) {
                        let repo = NodeRepo::new(name, node, &dirs);
                        if args.with_excluded {
                            repo.deploy_all()?;
                        } else {
                            repo.deploy()?;
                        }
                    }
                }
            }
        }
        Command::Git(args) => {
            let cluster: Cluster = read_config("cluster.toml", &dirs)?;
            let mut node_names = args[0].to_string_lossy().into_owned();
            node_names.retain(|c| !c.is_whitespace());
            let mut node_names: Vec<&str> = node_names.split(',').collect();
            node_names.dedup();

            for node_name in node_names {
                if node_name == "root" {
                    let root = RootRepo::from_cluster(&cluster, &dirs);
                    root.gitcall(args[1..].to_vec())?;
                } else {
                    let node = cluster.get_node(node_name)?;
                    let node = NodeRepo::new(node_name, node, &dirs);
                    node.gitcall(args[1..].to_vec())?;
                }
            }
        }
    }

    Ok(ExitCode::Success)
}

/// Standard exit codes.
#[derive(Debug)]
pub enum ExitCode {
    /// Nothing went wrong.
    Success,

    /// SNAFU!
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
