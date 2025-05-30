// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Command set implementation.
//!
//! This module is the forward facing API of internal library. It is meant to be used in `main` of
//! the OCD binary. The entire OCD command set is implemented right there!.

use crate::{
    glob_match,
    model::{
        config_dir, data_dir, Cluster, HookAction, HookKind, HookRunner, NodeEntry, RootEntry,
    },
    store::{DeployAction, MultiNodeClone, Node, Root, TablizeCluster},
};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use inquire::prompt_confirmation;
use std::{
    ffi::OsString,
    fs::{remove_dir_all, remove_file},
};
use tracing::{info, instrument, warn};

/// OCD public command set CLI.
#[derive(Debug, Clone, Parser)]
#[command(
    about,
    override_usage = "\n  ocd [options] <ocd-command>\n  ocd [options] [target]... <git-command>",
    subcommand_help_heading = "Commands",
    version,
)]
pub struct Ocd {
    #[arg(default_value_t = HookAction::default(), long, short, value_enum, value_name = "action")]
    pub run_hook: HookAction,

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
            Command::Clone(opts) => run_clone(self.run_hook, opts).await,
            Command::Init(opts) => run_init(self.run_hook, opts),
            Command::Deploy(opts) => run_deploy(self.run_hook, opts),
            Command::Undeploy(opts) => run_undeploy(self.run_hook, opts),
            Command::Remove(opts) => run_remove(self.run_hook, opts),
            Command::List(opts) => run_list(opts),
            Command::Git(opts) => run_git(opts),
        }
    }
}

/// Full command-set of OCD.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Clone existing cluster from root repository.
    #[command(override_usage = "ocd clone [options] <url>")]
    Clone(CloneOptions),

    /// Initialize new entries.
    #[command(override_usage = "ocd init [options] <node_name>")]
    Init(InitOptions),

    /// Deploy target entries in cluster.
    #[command(override_usage = "ocd deploy [options] [target]...")]
    Deploy(DeployOptions),

    /// Undeploy target entries in cluster.
    #[command(override_usage = "ocd undeploy [options] [target]...")]
    Undeploy(UndeployOptions),

    /// Remove target entries from cluster.
    #[command(name = "rm", override_usage = "ocd rm [options] [target]...")]
    Remove(RemoveOptions),

    /// List current entries in cluster.
    #[command(name = "ls", override_usage = "ocd list [options]")]
    List(ListOptions),

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

/// Initialize new entry in repository store, based on cluster configuration entry.
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct InitOptions {
    /// Name of new repository to initialize.
    #[arg(value_name = "entry_name")]
    pub entry_name: String,
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

/// List current entries in cluster.
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct ListOptions {
    /// Only list names of each entry only.
    #[arg(short, long)]
    pub names_only: bool,
}

#[instrument(skip(opts), level = "debug")]
async fn run_clone(action: HookAction, opts: CloneOptions) -> Result<()> {
    // INVARIANT: Wipe out cluster if root cannot be cloned or deployed.
    if let Err(error) = Root::new_clone(&opts.url) {
        warn!("Root clone failure, clearing broken cluster");
        let config_dir = config_dir()?;
        if config_dir.exists() {
            remove_dir_all(&config_dir)
                .with_context(|| format!("Failed to remove {config_dir:?}"))?;
        }

        let data_dir = data_dir()?;
        if data_dir.exists() {
            remove_dir_all(&data_dir).with_context(|| format!("Failed to remove {data_dir:?}"))?;
        }

        return Err(error);
    }

    let cluster = Cluster::new()?;
    let mut hooks = HookRunner::new()?;
    hooks.set_action(action);

    hooks.run("clone", HookKind::Pre, None)?;
    let multi_clone = MultiNodeClone::new(&cluster, opts.jobs)?;
    multi_clone.clone_all().await?;
    hooks.run("clone", HookKind::Post, None)?;

    Ok(())
}

pub fn run_init(action: HookAction, opts: InitOptions) -> Result<()> {
    let mut hooks = HookRunner::new()?;
    hooks.set_action(action);
    hooks.run("init", HookKind::Pre, None)?;

    match opts.entry_name.as_str() {
        "root" => {
            let path = config_dir()?.join(format!("{}.toml", opts.entry_name));
            if !path.exists() {
                return Err(anyhow!("No root entry to initialize! Define {path:?} first!"));
            }

            let data = std::fs::read_to_string(path)?;
            let root: RootEntry = toml::de::from_str(&data)?;
            let _ = Root::new_init(&root)?;
        }
        &_ => {
            let cluster = Cluster::new()?;
            let _ = Root::new_open(&cluster.root)
                .with_context(|| "Root may not have been properly initialized")?;

            let path = config_dir()?.join("nodes").join(format!("{}.toml", opts.entry_name));
            if !path.exists() {
                return Err(anyhow!("No node entry to initialize! Define {path:?} first!"));
            }

            let data = std::fs::read_to_string(path)?;
            let node: NodeEntry = toml::de::from_str(&data)?;
            let _ = Node::new_init(&opts.entry_name, &node)?;
        }
    }

    hooks.run("init", HookKind::Post, None)?;

    Ok(())
}

#[instrument(skip(opts), level = "debug")]
pub fn run_deploy(run_hook: HookAction, mut opts: DeployOptions) -> Result<()> {
    let cluster = Cluster::new()?;
    let root = Root::new_open(&cluster.root)?;
    let action = if opts.with_excluded { DeployAction::DeployAll } else { DeployAction::Deploy };

    let mut hooks = HookRunner::new()?;
    hooks.set_action(run_hook);

    opts.patterns.dedup();
    for pattern in &mut opts.patterns {
        pattern.retain(|c| !c.is_whitespace());
    }

    if let Some(index) = opts.patterns.iter().position(|x| *x == "root") {
        opts.patterns.swap_remove(index);
        root.deploy(action)?;
    }

    let targets = glob_match(&opts.patterns, cluster.nodes.keys());
    hooks.run("deploy", HookKind::Pre, Some(&targets))?;

    let mut nodes = Vec::new();
    if opts.only {
        for target in &targets {
            let entry = cluster.nodes.get(target).ok_or(anyhow!("Node {target:?} not defined"))?;
            let node = Node::new_open(target, entry)?;
            nodes.push(node);
        }
    } else {
        for target in &targets {
            for (name, entry) in cluster.dependency_iter(target) {
                let node = Node::new_open(name, entry)?;
                nodes.push(node);
            }
        }
    }

    for node in nodes {
        node.deploy(action)?;
    }

    hooks.run("deploy", HookKind::Post, Some(&targets))?;

    Ok(())
}

fn run_undeploy(run_hook: HookAction, mut opts: UndeployOptions) -> Result<()> {
    let cluster = Cluster::new()?;
    let root = Root::new_open(&cluster.root)?;

    let action =
        if opts.excluded_only { DeployAction::UndeployExcludes } else { DeployAction::Undeploy };

    let mut hooks = HookRunner::new()?;
    hooks.set_action(run_hook);

    opts.patterns.dedup();
    for pattern in &mut opts.patterns {
        pattern.retain(|c| !c.is_whitespace());
    }

    if let Some(index) = opts.patterns.iter().position(|x| *x == "root") {
        opts.patterns.swap_remove(index);
        root.deploy(action)?;
    }

    let targets = glob_match(&opts.patterns, cluster.nodes.keys());
    hooks.run("undeploy", HookKind::Pre, Some(&targets))?;

    let mut nodes = Vec::new();
    if opts.only {
        for target in &targets {
            let entry = cluster.nodes.get(target).ok_or(anyhow!("Node {target:?} not defined"))?;
            let node = Node::new_open(target, entry)?;
            nodes.push(node);
        }
    } else {
        for target in &targets {
            for (name, entry) in cluster.dependency_iter(target) {
                let node = Node::new_open(name, entry)?;
                nodes.push(node);
            }
        }
    }

    for node in nodes {
        node.deploy(action)?;
    }

    hooks.run("undeploy", HookKind::Post, Some(&targets))?;

    Ok(())
}

#[instrument(skip(opts), level = "debug")]
fn run_remove(run_hook: HookAction, mut opts: RemoveOptions) -> Result<()> {
    let cluster = Cluster::new()?;
    let mut hooks = HookRunner::new()?;
    hooks.set_action(run_hook);

    opts.patterns.dedup();
    for pattern in &mut opts.patterns {
        pattern.retain(|c| !c.is_whitespace());
    }

    if let Some(index) = opts.patterns.iter().position(|x| *x == "root") {
        warn!("Removing root will nuke your entire cluster");
        if prompt_confirmation("Do you want to send your cluster to the gallows? [y/n]")? {
            nuke_cluster(&cluster)?;
            return Ok(());
        } else {
            opts.patterns.swap_remove(index);
        }
    }

    let targets = glob_match(&opts.patterns, cluster.nodes.keys());
    hooks.run("rm", HookKind::Pre, Some(&targets))?;

    for target in &targets {
        let node = cluster.nodes.get(target).ok_or(anyhow!("Node {target:?} not defined"))?;
        let repo = Node::new_open(target, node)?;
        repo.deploy(DeployAction::Undeploy)?;
        remove_file(config_dir()?.join("nodes").join(format!("{target}.toml")))?;
        remove_dir_all(repo.path())?;
    }

    hooks.run("rm", HookKind::Post, Some(&targets))?;

    Ok(())
}

fn nuke_cluster(cluster: &Cluster) -> Result<()> {
    let root = Root::new_open(&cluster.root)?;
    root.nuke()?;

    for (name, node) in &cluster.nodes {
        if !data_dir()?.join(name).exists() {
            warn!("Node {name:?} not found in repository store");
            continue;
        }

        let repo = Node::new_open(name, node)?;
        repo.nuke()?;
    }

    remove_dir_all(config_dir()?)?;
    info!("Configuration directory removed");

    remove_dir_all(data_dir()?)?;
    info!("Data directory removed");

    Ok(())
}

fn run_list(opts: ListOptions) -> Result<()> {
    let cluster = Cluster::new()?;
    let root = Root::new_open(&cluster.root)?;

    let tablize = TablizeCluster::new(&root, &cluster);
    if opts.names_only {
        tablize.names_only()?;
    } else {
        tablize.fancy()?;
    }

    Ok(())
}

fn run_git(opts: Vec<OsString>) -> Result<()> {
    let cluster = Cluster::new()?;
    let mut patterns = opts[0].to_string_lossy().into_owned();
    patterns.retain(|c| !c.is_whitespace());
    let mut patterns: Vec<&str> = patterns.split(',').collect();
    patterns.dedup();

    if let Some(index) = patterns.iter().position(|x| *x == "root") {
        patterns.swap_remove(index);
        let root = Root::new_open(&cluster.root)?;
        root.gitcall(opts[1..].to_vec())?;
    }

    let targets = glob_match(patterns, cluster.nodes.keys());
    for target in &targets {
        let node = cluster.nodes.get(target).ok_or(anyhow!("{target} not found"))?;
        let node = Node::new_open(target, node)?;
        node.gitcall(opts[1..].to_vec())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use clap::CommandFactory;

    #[test]
    fn cli_verify_structure() {
        Ocd::command().debug_assert();
    }
}
