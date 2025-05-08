// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Command set implementation.
//!
//! This module is the forward facing API of internal library. It is meant to be used in `main` of
//! the OCD binary. The entire OCD command set is implemented right there!.

use crate::{
    fs::{config_dir, data_dir, home_dir, load, save, Existence},
    glob_match,
    model::{Cluster, DeploymentKind, DirAlias, HookKind, HookRunner, NodeEntry},
    store::{DeployAction, MultiNodeClone, Node, Root, TablizeCluster},
    Error, Result,
};

use clap::{Parser, Subcommand, ValueEnum};
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

/// Behavior variants for hook execution.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum HookAction {
    /// Always execute hooks no questions asked.
    Always,

    /// Prompt user with hook's contents, and execute it if and only if user accepts it.
    #[default]
    Prompt,

    /// Never execute hooks no questions asked.
    Never,
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

/// List current entries in cluster.
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct ListOptions {
    /// Only list names of each entry only.
    #[arg(short, long)]
    pub names_only: bool,
}

async fn run_clone(run_hook: HookAction, opts: CloneOptions) -> Result<()> {
    let _ = match Root::new_clone(&opts.url) {
        Ok(root) => root,
        Err(error) => {
            // INVARIANT: Wipe out cluster if root cannot be cloned or deployed.
            remove_dir_all(data_dir()?)?;
            remove_dir_all(config_dir()?)?;
            return Err(error);
        }
    };

    let mut hooks: HookRunner = load("hooks.toml", Existence::NotRequired)?;
    hooks.set_action(run_hook);
    hooks.run("clone", HookKind::Pre, None)?;

    let cluster: Cluster = load("cluster.toml", Existence::Required)?;
    let multi_clone = MultiNodeClone::new(&cluster, opts.jobs)?;
    multi_clone.clone_all().await?;

    hooks.run("clone", HookKind::Post, None)?;

    Ok(())
}

pub fn run_init(run_hook: HookAction, opts: InitOptions) -> Result<()> {
    let mut cluster: Cluster = load("cluster.toml", Existence::Required)?;

    let mut hooks: HookRunner = load("hooks.toml", Existence::NotRequired)?;
    hooks.set_action(run_hook);
    hooks.run("init", HookKind::Pre, None)?;

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

        let node = NodeEntry { deployment, ..Default::default() };
        let name = opts.node_name.as_ref().ok_or(Error::NoNodeName)?;
        let _ = Node::new_init(name, &node)?;

        cluster.add_node(name, node)?;
    }

    save("cluster.toml", cluster.to_string())?;

    hooks.run("init", HookKind::Post, None)?;

    Ok(())
}

#[instrument(skip(opts), level = "debug")]
pub fn run_deploy(run_hook: HookAction, mut opts: DeployOptions) -> Result<()> {
    let root = Root::new_open()?;
    let cluster: Cluster = load("cluster.toml", Existence::Required)?;

    let mut hooks: HookRunner = load("hooks.toml", Existence::NotRequired)?;
    hooks.set_action(run_hook);

    let action = if opts.with_excluded { DeployAction::DeployAll } else { DeployAction::Deploy };

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
            let entry = cluster.get_node(target)?;
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
    let root = Root::new_open()?;
    let cluster: Cluster = load("cluster.toml", Existence::Required)?;

    let mut hooks: HookRunner = load("hooks.toml", Existence::NotRequired)?;
    hooks.set_action(run_hook);

    let action =
        if opts.excluded_only { DeployAction::UndeployExcludes } else { DeployAction::Undeploy };

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
            let entry = cluster.get_node(target)?;
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
    let mut cluster: Cluster = load("cluster.toml", Existence::Required)?;

    let mut hooks: HookRunner = load("hooks.toml", Existence::NotRequired)?;
    hooks.set_action(run_hook);

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
    hooks.run("rm", HookKind::Pre, Some(&targets))?;

    for target in &targets {
        let node = cluster.remove_node(target)?;
        let repo = Node::new_open(target, &node)?;
        repo.deploy(DeployAction::Undeploy)?;
        remove_dir_all(repo.path())?;
    }

    save("cluster.toml", cluster.to_string())?;

    hooks.run("rm", HookKind::Post, Some(&targets))?;

    Ok(())
}

fn run_list(opts: ListOptions) -> Result<()> {
    let root = Root::new_open()?;
    let cluster: Cluster = load("cluster.toml", Existence::Required)?;

    let tablize = TablizeCluster::new(&root, &cluster);
    if opts.names_only {
        tablize.names_only()?;
    } else {
        tablize.fancy()?;
    }

    Ok(())
}

fn run_git(opts: Vec<OsString>) -> Result<()> {
    let cluster: Cluster = load("cluster.toml", Existence::Required)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    use clap::CommandFactory;

    #[test]
    fn cli_verify_structure() {
        Ocd::command().debug_assert();
    }
}
