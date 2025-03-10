// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

#![allow(dead_code)]

mod config;
mod repo;

use crate::{
    config::{read_config, Cluster, Layout},
    repo::{MultiClone, NodeRepo, RootRepo},
};

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::{ffi::OsString, fs::remove_dir_all, process};

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

    #[command(override_usage = "ocd deploy [options] [node_names]...")]
    Deploy(DeployOptions),

    #[command(override_usage = "ocd undeploy [options] [node_names]...")]
    Undeploy(UndeployOptions),

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

/// Undeploy nodes of cluster.
#[derive(Args, Debug)]
pub struct UndeployOptions {
    /// List of nodes to undeploy.
    #[arg(value_parser, num_args = 1.., value_delimiter = ',', value_name = "node_names")]
    pub node_names: Vec<String>,

    /// Do not undeploy dependencies of target nodes.
    #[arg(short, long)]
    pub only: bool,

    /// Undeploy excluded files only.
    #[arg(short, long)]
    pub excluded_only: bool,
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
        Command::Deploy(mut args) => {
            let (root, cluster) = if !layout.config_dir().join("cluster.toml").exists() {
                let root = RootRepo::new_open(&layout)?;
                let cluster = root.get_cluster()?;
                root.deploy()?;
                (root, cluster)
            } else {
                let cluster: Cluster = read_config("cluster.toml", &layout)?;
                let root = RootRepo::from_cluster(&cluster, &layout);
                (root, cluster)
            };

            args.node_names.dedup();
            if let Some(index) = args.node_names.iter().position(|x| *x == "root") {
                args.node_names.swap_remove(index);
                if args.with_excluded {
                    root.deploy_all()?;
                } else {
                    root.deploy()?;
                }
            }

            for mut node_name in args.node_names {
                node_name.retain(|c| !c.is_whitespace());
                if args.only {
                    let (name, node) = cluster.get_node(node_name)?;
                    let repo = NodeRepo::from_node(name, node, &layout);
                    if args.with_excluded {
                        repo.deploy_all()?;
                    } else {
                        repo.deploy()?;
                    }
                } else {
                    for (name, node) in cluster.dependency_iter(node_name)? {
                        let repo = NodeRepo::from_node(name, node, &layout);
                        if args.with_excluded {
                            repo.deploy_all()?;
                        } else {
                            repo.deploy()?;
                        }
                    }
                }
            }
        }
        Command::Undeploy(mut args) => {
            let (root, cluster) = if !layout.config_dir().join("cluster.toml").exists() {
                let root = RootRepo::new_open(&layout)?;
                let cluster = root.get_cluster()?;
                root.deploy()?;
                (root, cluster)
            } else {
                let cluster: Cluster = read_config("cluster.toml", &layout)?;
                let root = RootRepo::from_cluster(&cluster, &layout);
                (root, cluster)
            };

            args.node_names.dedup();
            if let Some(index) = args.node_names.iter().position(|x| *x == "root") {
                args.node_names.swap_remove(index);
                if args.excluded_only {
                    root.undeploy_excludes()?;
                } else {
                    log::warn!("cannot undeploy root");
                }
            }

            for mut node_name in args.node_names {
                node_name.retain(|c| !c.is_whitespace());
                if args.only {
                    let (name, node) = cluster.get_node(node_name)?;
                    let repo = NodeRepo::from_node(name, node, &layout);
                    if args.excluded_only {
                        repo.undeploy_excludes()?;
                    } else {
                        repo.undeploy()?;
                    }
                } else {
                    for (name, node) in cluster.dependency_iter(node_name)? {
                        let repo = NodeRepo::from_node(name, node, &layout);
                        if args.excluded_only {
                            repo.undeploy_excludes()?;
                        } else {
                            repo.undeploy()?;
                        }
                    }
                }
            }
        }
        Command::Git(args) => {
            let cluster: Cluster = read_config("cluster.toml", &layout)?;
            let mut node_names = args[0].to_string_lossy().into_owned();
            node_names.retain(|c| !c.is_whitespace());
            let mut node_names: Vec<&str> = node_names.split(',').collect();
            node_names.dedup();

            for node_name in node_names {
                if node_name == "root" {
                    let root = RootRepo::from_cluster(&cluster, &layout);
                    root.git_bin(args[1..].to_vec())?;
                } else {
                    let (name, node) = cluster.get_node(node_name)?;
                    let node = NodeRepo::from_node(name, node, &layout);
                    node.git_bin(args[1..].to_vec())?;
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
