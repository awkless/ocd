// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Command set implementation.
//!
//! This module is the forward facing API of internal library. It is meant to be used in `main` of
//! the OCD binary. The entire OCD command set is implemented right there!.

use crate::{
    fs::{read_to_config, write_to_config},
    model::{Cluster, DeploymentKind, DirAlias, NodeEntry},
    path::{config_dir, data_dir, home_dir},
    store::{DeployAction, MultiNodeClone, Node, Root},
    utils::glob_match,
    Error, Result,
};

use clap::{Parser, Subcommand};
use inquire::prompt_confirmation;
use std::{ffi::OsString, fs::remove_dir_all, path::PathBuf};
use tracing::{instrument, warn};

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
    pub async fn run(self) -> Result<()> {
        match self.command {
            Command::Clone(opts) => run_clone(opts).await,
            Command::Init(opts) => run_init(opts),
            Command::Deploy(opts) => run_deploy(opts),
            Command::Undeploy(opts) => run_undeploy(opts),
            Command::Remove(opts) => run_remove(opts),
            Command::Git(opts) => run_git(opts),
        }
    }
}

/// Full command-set of OCD.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Clone existing cluster.
    #[command(override_usage = "ocd clone [options] <url>")]
    Clone(CloneOptions),

    /// Initialize new repository.
    #[command(override_usage = "ocd init [options] <node_name>")]
    Init(InitOptions),

    /// Deploy node of cluster.
    #[command(override_usage = "ocd deploy [options] [pattern]...")]
    Deploy(DeployOptions),

    /// Undeploy nodes of cluster.
    #[command(override_usage = "ocd undeploy [options] [pattern]...")]
    Undeploy(UndeployOptions),

    /// Remove target node from cluster.
    #[command(name = "rm", override_usage = "ocd rm [options] [pattern]...")]
    Remove(RemoveOptions),

    /// Git binary shortcut.
    #[command(external_subcommand)]
    Git(Vec<OsString>),
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

/// Initialize new repository.
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct InitOptions {
    /// Name of new repository to initialize.
    #[arg(group = "entry", value_name = "node_name")]
    pub node_name: Option<String>,

    /// Mark repository as root of cluster.
    #[arg(groups = ["entry", "kind"], short, long)]
    pub root: bool,

    /// Mark directory as worktree-alias (makes node repositories bare-alias).
    #[arg(group = "node", short, long, value_name = "dir_path")]
    pub dir_alias: Option<PathBuf>,

    /// Mark node repository as bare-alias, and default worktree-alias to $HOME.
    #[arg(groups = ["node", "kind"], short = 'H', long)]
    pub home_alias: bool,
}

/// Deploy node of cluster.
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct DeployOptions {
    /// List of nodes to deploy ("root" is always deployed).
    #[arg(value_parser, num_args = 1.., value_delimiter = ',', value_name = "pattern")]
    pub patterns: Vec<String>,

    /// Do not deploy dependencies of target nodes.
    #[arg(short, long)]
    pub only: bool,

    /// Deploy excluded files as well.
    #[arg(short, long)]
    pub with_excluded: bool,
}

/// Undeploy nodes of cluster.
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct UndeployOptions {
    /// List of nodes to undeploy ("root" cannot be undeployed).
    #[arg(value_parser, num_args = 1.., value_delimiter = ',', value_name = "pattern")]
    pub patterns: Vec<String>,

    /// Do not undeploy dependencies of target nodes.
    #[arg(short, long)]
    pub only: bool,

    /// Undeploy excluded files only.
    #[arg(short, long)]
    pub excluded_only: bool,
}

/// Remove target node from cluster.
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct RemoveOptions {
    /// List of nodes to remove ("root" will nuke cluster).
    #[arg(value_parser, num_args = 1.., value_delimiter = ',', value_name = "pattern")]
    pub patterns: Vec<String>,
}

async fn run_clone(opts: CloneOptions) -> Result<()> {
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

pub fn run_init(opts: InitOptions) -> Result<()> {
    let mut cluster: Cluster = read_to_config(config_dir()?.join("cluster.toml"))?;

    if opts.root {
        let _ = Root::new_init()?;
    } else {
        // INVARIANT: Make sure root always exists.
        if !data_dir()?.join("root").exists() {
            let _ = Root::new_init()?;
        }

        let deployment = if opts.home_alias {
            DeploymentKind::BareAlias(DirAlias::new(home_dir()?))
        } else if opts.dir_alias.is_some() {
            DeploymentKind::BareAlias(DirAlias::new(opts.dir_alias.unwrap()))
        } else {
            DeploymentKind::Normal
        };

        let node = NodeEntry {
            deployment,
            ..Default::default()
        };
        let name = opts.node_name.as_ref().ok_or(Error::NoNodeName)?;
        let _ = Node::new_init(name, &node)?;

        cluster.add_node(name, node)?;
    }

    write_to_config(config_dir()?.join("cluster.toml"), cluster.to_string())?;

    Ok(())
}

pub fn run_deploy(mut opts: DeployOptions) -> Result<()> {
    let root = Root::new_open()?;
    let cluster: Cluster = read_to_config(config_dir()?.join("cluster.toml"))?;
    let action = if opts.with_excluded {
        DeployAction::DeployAll
    } else {
        DeployAction::Deploy
    };

    opts.patterns.dedup();
    for pattern in &mut opts.patterns {
        pattern.retain(|c| !c.is_whitespace());
    }

    if let Some(index) = opts.patterns.iter().position(|x| *x == "root") {
        opts.patterns.swap_remove(index);
        root.deploy(action)?;
    }

    let targets = glob_match(&opts.patterns, cluster.nodes.keys());
    for target in &targets {
        if opts.only {
            let entry = cluster.get_node(target)?;
            let node = Node::new_open(target, entry)?;
            node.deploy(action)?;
        } else {
            for (name, entry) in cluster.dependency_iter(target) {
                let node = Node::new_open(name, entry)?;
                node.deploy(action)?;
            }
        }
    }

    Ok(())
}

fn run_undeploy(mut opts: UndeployOptions) -> Result<()> {
    let root = Root::new_open()?;
    let cluster: Cluster = read_to_config(config_dir()?.join("cluster.toml"))?;
    let action = if opts.excluded_only {
        DeployAction::UndeployExcludes
    } else {
        DeployAction::Undeploy
    };

    opts.patterns.dedup();
    for pattern in &mut opts.patterns {
        pattern.retain(|c| !c.is_whitespace());
    }

    if let Some(index) = opts.patterns.iter().position(|x| *x == "root") {
        opts.patterns.swap_remove(index);
        root.deploy(action)?;
    }

    let targets = glob_match(&opts.patterns, cluster.nodes.keys());
    for target in &targets {
        if opts.only {
            let entry = cluster.get_node(target)?;
            let node = Node::new_open(target, entry)?;
            node.deploy(action)?;
        } else {
            for (name, entry) in cluster.dependency_iter(target) {
                let node = Node::new_open(name, entry)?;
                node.deploy(action)?;
            }
        }
    }

    Ok(())
}

#[instrument(skip(opts))]
fn run_remove(mut opts: RemoveOptions) -> Result<()> {
    let mut cluster: Cluster = read_to_config(config_dir()?.join("cluster.toml"))?;

    opts.patterns.dedup();
    for pattern in &mut opts.patterns {
        pattern.retain(|c| !c.is_whitespace());
    }

    if let Some(index) = opts.patterns.iter().position(|x| *x == "root") {
        warn!("Removing root will nuke your entire cluster");
        if prompt_confirmation("Do you want to send your cluster to the gallows? [y/n]")? {
            let root = Root::new_open()?;
            root.nuke()?;
            return Ok(());
        } else {
            opts.patterns.swap_remove(index);
        }
    }

    let targets = glob_match(&opts.patterns, cluster.nodes.keys());
    for target in &targets {
        let node = cluster.remove_node(target)?;
        let repo = Node::new_open(target, &node)?;
        repo.deploy(DeployAction::Undeploy)?;
        remove_dir_all(repo.path())?;
    }

    write_to_config(config_dir()?.join("cluster.toml"), cluster.to_string())?;

    Ok(())
}

fn run_git(opts: Vec<OsString>) -> Result<()> {
    let cluster: Cluster = read_to_config(config_dir()?.join("cluster.toml"))?;
    let mut patterns = opts[0].to_string_lossy().into_owned();
    patterns.retain(|c| !c.is_whitespace());
    let mut patterns: Vec<&str> = patterns.split(',').collect();
    patterns.dedup();

    if let Some(index) = patterns.iter().position(|x| *x == "root") {
        patterns.swap_remove(index);
        let root = Root::new_open()?;
        root.gitcall(opts[1..].to_vec())?;
    }

    let targets = glob_match(patterns, cluster.nodes.keys());
    for target in &targets {
        let node = cluster.get_node(target)?;
        let node = Node::new_open(target, node)?;
        node.gitcall(opts[1..].to_vec())?;
    }

    Ok(())
}
