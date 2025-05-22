// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Configuration model.
//!
//! Handles the parsing, deserialization, and overall management of configuration file data for the
//! OCD tool.

use anyhow::{anyhow, Context, Result};
use beau_collector::BeauCollector as _;
use clap::ValueEnum;
use config::{Config, File};
use minus::{
    input::{HashedEventRegister, InputEvent},
    page_all, ExitStrategy, LineNumbers, Pager,
};
use run_script::{run_script, ScriptOptions};
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsString,
    fmt,
    fs::read_to_string,
    hash::RandomState,
    marker::PhantomData,
    path::{Path, PathBuf},
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
};
use tracing::{debug, info, instrument, trace, warn};

/// Cluster definition handler.
///
/// A cluster definition simply defines the entries of a given cluster that OCD must manage through
/// the repository store. A cluster is comprised of two basic entry types: __root__ and __node__.
/// The root is always bare-alias, and is always deployed, because it contains the cluster
/// definition itself. There can only be one root for any given cluster that the user defines.
/// A node entry can either be normal or bare-alias. The user can define zero or more nodes within
/// a given cluster, while root must always exist.
///
/// Each entry in the cluster definition receives its own configuration file in the TOML format.
/// The root of a cluster is stored at the top-level of OCD's configuration directory as
/// `$XDG_CONFIG_HOME/ocd/root.toml`, while nodes are stored in a sub-directory at
/// `$XDG_CONFIG_HOME/ocd/nodes`. The name of a given configuration file is the name that it will
/// be given within the repository store (excluding file extension).
///
/// # Invariants
///
/// - Root always exists.
/// - All node dependencies are acyclic.
/// - Working directory aliases are expanded.
/// - Node dependencies are defined.
#[derive(Debug, PartialEq, Eq)]
pub struct Cluster {
    /// Root entry of cluster.
    pub root: RootEntry,

    /// Node entries of cluster represented as DAG.
    pub nodes: HashMap<String, NodeEntry>,
}

impl Cluster {
    /// Construct new cluster definition by reading and deserializing configuration files.
    ///
    /// # Errors
    ///
    /// - Will fail if `root.toml` does not exist.
    /// - Will fail if _any_ configuration file contains invalid TOML formatting.
    #[instrument(level = "debug")]
    pub fn new() -> Result<Self> {
        trace!("Load cluster configuration");

        let path = config_dir()?.join("root.toml");
        debug!("Load root at {path:?}");
        let root: RootEntry =
            Config::builder().add_source(File::from(path)).build()?.try_deserialize()?;
        let pattern = config_dir()?.join("nodes").join("*.toml").to_string_lossy().into_owned();
        let mut nodes = HashMap::new();
        for entry in glob::glob(pattern.as_str())? {
            // INVARIANT: The name of a node is the file name itself without the extension.
            let path = entry?;
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();

            debug!("Load node {name:?} at {path:?}");
            let node: NodeEntry = Config::builder()
                .add_source(File::from(path).required(false))
                .build()?
                .try_deserialize()?;
            nodes.insert(name, node);
        }

        let mut cluster = Self { root, nodes };
        cluster.dependency_existence_check()?;
        cluster.acyclic_check()?;
        cluster.expand_work_dir_aliases()?;

        Ok(cluster)
    }

    /// Iterate through node dependencies of target node entry inclusively.
    ///
    /// There is no specific ordering for node dependencies being iterated through.
    pub fn dependency_iter(&self, node: impl Into<String>) -> DependencyIter<'_> {
        let mut stack = VecDeque::new();
        stack.push_front(node.into());
        DependencyIter { graph: &self.nodes, visited: HashSet::new(), stack }
    }

    #[instrument(skip(self), level = "debug")]
    fn dependency_existence_check(&self) -> Result<()> {
        trace!("Perform dependency existence check on cluster");
        let mut results = Vec::new();
        for node in self.nodes.values() {
            for dependency in node.settings.dependencies.iter().flatten() {
                if !self.nodes.contains_key(dependency) {
                    results.push(Err(anyhow!(
                        "Node dependency {dependency:?} is not defined in cluster"
                    )));
                } else {
                    results.push(Ok(()));
                }
            }
        }

        results.into_iter().bcollect::<_>()
    }

    #[instrument(skip(self), level = "debug")]
    fn acyclic_check(&self) -> Result<()> {
        trace!("Perform acyclic check on cluster");
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        // INVARIANT: The in-degree of a node is the sum all all incoming edegs of each
        // destination node.
        for (name, node) in &self.nodes {
            in_degree.entry(name.clone()).or_insert(0);
            for dependency in node.settings.dependencies.iter().flatten() {
                *in_degree.entry(dependency.clone()).or_insert(0) += 1;
            }
        }

        // INVARIANT: Queue only contains nodes with in-degree of 0.
        for (name, degree) in &in_degree {
            if *degree == 0 {
                queue.push_back(name.clone());
            }
        }

        while let Some(current) = queue.pop_front() {
            for dependency in self.nodes[&current].settings.dependencies.iter().flatten() {
                *in_degree.get_mut(dependency).unwrap() -= 1;
                if *in_degree.get(dependency).unwrap() == 0 {
                    queue.push_back(dependency.clone());
                }
            }
            visited.insert(current);
        }

        // INVARIANT: Queue is empty, but graph has not been fully visited.
        //   - There exists a cycle.
        //   - The unvisited nodes represent this cycle.
        if visited.len() != self.nodes.len() {
            let cycle: Vec<String> =
                self.nodes.keys().filter(|key| !visited.contains(*key)).cloned().collect();
            return Err(anyhow!("Cluster contains cycle(s): {cycle:?}"));
        }
        debug!("Topological sort of cluster nodes: {visited:?}");

        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    fn expand_work_dir_aliases(&mut self) -> Result<()> {
        trace!("Expand working directory aliases of nodes");
        for node in self.nodes.values_mut() {
            let expand = shellexpand::full(
                node.settings.deployment.work_dir_alias.0.to_string_lossy().as_ref(),
            )?
            .into_owned();
            node.settings.deployment.work_dir_alias = WorkDirAlias::new(expand);
        }
        Ok(())
    }
}

/// Iterator for node entry dependencies.
///
/// Obtain a full listing of nodes defined as dependencies of a given target node that was
/// initially pushed into stack.
#[derive(Debug)]
pub struct DependencyIter<'cluster> {
    graph: &'cluster HashMap<String, NodeEntry>,
    visited: HashSet<String>,
    stack: VecDeque<String>,
}

impl<'cluster> Iterator for DependencyIter<'cluster> {
    type Item = (&'cluster str, &'cluster NodeEntry);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = self.stack.pop_front() {
            let (name, node) = self.graph.get_key_value(&node)?;
            for dependency in node.settings.dependencies.iter().flatten() {
                if !self.visited.contains(dependency) {
                    self.stack.push_front(dependency.clone());
                    self.visited.insert(dependency.clone());
                }
            }
            return Some((name.as_str(), node));
        }
        None
    }
}

/// Root entry of cluster definition.
///
/// Any and all cluster's that OCD operates on must have a _root_. The root contains the cluster
/// definition, which must always be deployed in order for OCD to know what nodes it must manage
/// within the repository store.
///
/// Root is always bare-alias such that it can only be deployed at two locations relative to the
/// user's home directory:
///
/// 1. The user's home directory itself.
/// 2. The standard configuration directory for OCD, i.e., `$XDG_CONFIG_HOME/ocd`.
///
/// This restriction of deployment for root ensures that the cluster definition always exists at
/// the standard configuration directory. This ensures that the cluster definition can be reliably
/// read, parsed, and deserialized during runtime. This also ensures that OCD can easily clone a
/// target cluster and deploy it by simply using root itself.
///
/// Root also has access to the file exclusion feature. The user can specify a list of sparsity
/// rules to exclude certain files and directories from deployment.
#[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
pub struct RootEntry {
    /// Deployment options.
    pub settings: RootEntrySettings,
}

impl RootEntry {
    /// Construct new root entry through builder.
    ///
    /// # Errors
    ///
    /// Will fail if configuration directory path cannot be determined.
    pub fn builder() -> Result<RootEntryBuilder> {
        RootEntryBuilder::new()
    }

    /// Use default settings for root entry.
    ///
    /// # Errors
    ///
    /// Will fail if configuration directory cannot be determined.
    pub fn try_default() -> Result<Self> {
        Ok(RootEntry {
            settings: RootEntrySettings {
                work_dir_alias: WorkDirAlias::new(config_dir()?),
                excluded: None,
            },
        })
    }
}

/// Builder for [`RootEntry`].
#[derive(Debug)]
pub struct RootEntryBuilder {
    settings: RootEntrySettings,
}

impl RootEntryBuilder {
    /// Construct new builder for [`RootEntry`].
    ///
    /// # Errors
    ///
    /// Will fail if configuration directory cannot be determined.
    pub fn new() -> Result<Self> {
        Ok(Self {
            settings: RootEntrySettings {
                work_dir_alias: WorkDirAlias::new(config_dir()?),
                excluded: None,
            },
        })
    }

    /// Deploy to standard configuration directory.
    ///
    /// # Errors
    ///
    /// - Will fail if configuration directory path cannot be determined.
    pub fn deploy_to_config_dir(mut self) -> Result<Self> {
        self.settings.work_dir_alias = WorkDirAlias::new(config_dir()?);
        Ok(self)
    }

    /// Deploy to home directory.
    ///
    /// # Errors
    ///
    /// - Will fail if home directory path cannot be determined.
    pub fn deploy_to_home_dir(mut self) -> Result<Self> {
        self.settings.work_dir_alias = WorkDirAlias::new(home_dir()?);
        Ok(self)
    }

    /// Set exclusion rules to exclude files from deployment for node entry.
    pub fn excluded(mut self, rules: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.settings.excluded = Some(rules.into_iter().map(Into::into).collect());
        self
    }

    /// Build new [`RootEntry`].
    pub fn build(self) -> RootEntry {
        RootEntry { settings: self.settings }
    }
}

/// Deployment options for root entry.
#[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
pub struct RootEntrySettings {
    /// Working directory alias option.
    #[serde(deserialize_with = "deserialize_root_work_dir_alias")]
    pub work_dir_alias: WorkDirAlias,

    /// List of sparsity rules to exclude files from deployment.
    pub excluded: Option<Vec<String>>,
}

fn deserialize_root_work_dir_alias<'de, D>(deserializer: D) -> Result<WorkDirAlias, D::Error>
where
    D: Deserializer<'de>,
{
    let result: String = Deserialize::deserialize(deserializer)?;
    match result.as_str() {
        "config_dir" => Ok(WorkDirAlias::new(config_dir().map_err(serde::de::Error::custom)?)),
        "home_dir" => Ok(WorkDirAlias::new(home_dir().map_err(serde::de::Error::custom)?)),
        _ => Err(anyhow!("Invalid deployment option for root")).map_err(serde::de::Error::custom),
    }
}

/// Node entry of cluster.
///
/// A cluster typically contains a series of nodes. A given node entry can either be normal or
/// bare-alias. If the user does not specify a working directory alias, then their home directory
/// will be used as the default. All nodes contain a URL that points to a remote repository. This
/// URL is mainly used to tell OCD where to clone the node itself if it is missing in the
/// repository store.
///
/// Node entries have access to the file exclusion feature, and dependency deployment feature.
/// Thus, each node can have a listing of sparsity rules to exclude files and directories from
/// deployment, and a listing of other nodes as dependencies that must be deployed with the node
/// itself.
#[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
pub struct NodeEntry {
    pub settings: NodeEntrySettings,
}

impl NodeEntry {
    /// Construct new node entry through builder.
    ///
    /// # Errors
    ///
    /// Will fail if default working directory alias cannot be determined.
    pub fn builder() -> Result<NodeEntryBuilder> {
        NodeEntryBuilder::new()
    }
}

/// Builder for [`NodeEntry`]
#[derive(Debug)]
pub struct NodeEntryBuilder {
    settings: NodeEntrySettings,
}

impl NodeEntryBuilder {
    /// Construct new empty builder for [`NodeEntry`].
    ///
    /// # Errors
    ///
    /// Will fail if default working directory alias cannot be determined.
    pub fn new() -> Result<Self> {
        Ok(Self {
            settings: NodeEntrySettings {
                deployment: NodeEntryDeployment {
                    kind: DeploymentKind::Normal,
                    work_dir_alias: WorkDirAlias::try_default()?,
                },
                url: String::default(),
                excluded: None,
                dependencies: None,
            },
        })
    }

    /// Set method of deployment for node entry.
    pub fn deployment(mut self, kind: DeploymentKind, work_dir_alias: WorkDirAlias) -> Self {
        self.settings.deployment = NodeEntryDeployment { kind, work_dir_alias };
        self
    }

    /// Set URL to clone node entry from.
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.settings.url = url.into();
        self
    }

    /// Set exclusion rules to exclude files from deployment for node entry.
    pub fn excluded(mut self, rules: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.settings.excluded = Some(rules.into_iter().map(Into::into).collect());
        self
    }

    /// Set dependencies to be deployed with node entry.
    pub fn dependencies(mut self, nodes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.settings.dependencies = Some(nodes.into_iter().map(Into::into).collect());
        self
    }

    /// Build new [`NodeEntry`].
    pub fn build(self) -> NodeEntry {
        NodeEntry { settings: self.settings }
    }
}

/// Settings for node entry.
#[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
pub struct NodeEntrySettings {
    /// Deployment method for node entry.
    #[serde(deserialize_with = "deserialize_node_deployment")]
    pub deployment: NodeEntryDeployment,

    /// URL to clone node entry from.
    pub url: String,

    /// List of sparisty rules to exclude files from deployment.
    pub excluded: Option<Vec<String>>,

    /// List of other nodes to be deployed as dependencies with this node entry.
    pub dependencies: Option<Vec<String>>,
}

/// Node deployment method.
///
/// Currently, there are only two kinds of node deployment:
///
/// 1. Normal deployment kind.
/// 2. Bare-alias deployment kind.
///
/// Normal deployment simply ensures that the node entry has been cloned into repository store.
/// Bare-alias deployment not only ensures that node entry has been cloned into repository store,
/// but is also properly deployed to target working directory alias.
#[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
pub struct NodeEntryDeployment {
    /// Deployment kind.
    pub kind: DeploymentKind,

    /// Working directory alias to use.
    pub work_dir_alias: WorkDirAlias,
}

impl FromStr for NodeEntryDeployment {
    type Err = anyhow::Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let (kind, work_dir_alias) = match data {
            "normal" => (DeploymentKind::Normal, WorkDirAlias::try_default()?),
            "bare_alias" => (DeploymentKind::BareAlias, WorkDirAlias::new(home_dir()?)),
            _ => return Err(anyhow!("Invalid deployment kind")),
        };

        Ok(NodeEntryDeployment { kind, work_dir_alias })
    }
}

struct NodeEntryDeploymentVisitor(PhantomData<fn() -> NodeEntryDeployment>);

impl<'de> Visitor<'de> for NodeEntryDeploymentVisitor {
    type Value = NodeEntryDeployment;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string or map")
    }

    fn visit_str<E>(self, value: &str) -> Result<NodeEntryDeployment, E>
    where
        E: serde::de::Error,
    {
        FromStr::from_str(value).map_err(serde::de::Error::custom)
    }

    fn visit_map<M>(self, map: M) -> Result<NodeEntryDeployment, M::Error>
    where
        M: MapAccess<'de>,
    {
        Deserialize::deserialize(serde::de::value::MapAccessDeserializer::new(map))
    }
}

fn deserialize_node_deployment<'de, D>(deserializer: D) -> Result<NodeEntryDeployment, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(NodeEntryDeploymentVisitor(PhantomData))
}

/// Variants of node deployment.
#[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum DeploymentKind {
    /// Node is normal, so make sure it got cloned.
    Normal,

    /// Node is bare-alias, make sure it got cloned, and is deployed to working directory alias.
    BareAlias,
}

impl DeploymentKind {
    pub fn is_bare_alias(&self) -> bool {
        match self {
            DeploymentKind::Normal => false,
            DeploymentKind::BareAlias => true,
        }
    }
}

/// Working directory alias path.
#[derive(Debug, PartialEq, Eq, Clone, Deserialize)]
pub struct WorkDirAlias(pub(crate) PathBuf);

impl WorkDirAlias {
    /// Construct new working directory alias based on provided path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Try to use default path.
    ///
    /// Default path is user's home directory.
    ///
    /// # Errors
    ///
    /// - Will fail if user home directory cannot be determined.
    pub fn try_default() -> Result<Self> {
        Ok(Self(home_dir()?))
    }

    pub fn to_os_string(&self) -> OsString {
        OsString::from(self.0.to_string_lossy().into_owned())
    }
}

/// Execute user defined command hooks.
///
/// # Invariants
///
/// Always expand working directory paths.
#[derive(Debug)]
pub struct HookRunner {
    entries: CommandHooks,
    action: HookAction,
    pager: HookPager,
}

impl HookRunner {
    /// Construct new hook runner by loading hook configuration file.
    ///
    /// Will not fail if hook configuration file is missing, because hooks are deemed optional.
    ///
    /// # Errors
    ///
    /// Will fail if hook configuration file cannot be read, or contains invalid TOML formatting.
    pub fn new() -> Result<Self> {
        trace!("Load hook configurations");

        let path = config_dir()?.join("hooks.toml");
        debug!("Load hooks at {path:?}");
        let entries: CommandHooks = Config::builder()
            .add_source(File::from(path).required(false))
            .build()?
            .try_deserialize()?;

        Ok(Self { entries, action: HookAction::default(), pager: HookPager::default() })
    }

    /// Set hook action type.
    pub fn set_action(&mut self, action: HookAction) {
        self.action = action;
    }

    /// Run all hooks targeting a specific command and repositories in cluster.
    ///
    /// Skips empty hook entry listing, or hooks that match a specific target. Issues pager to both
    /// show what the contents of a given hook looks like, and prompts about its execution when
    /// using the prompt hook action. Otherwise, it will either execute hooks with no prompting, or
    /// not execut any hooks based on hook action.
    ///
    /// # Errors
    ///
    /// - Will fail for any pager failure.
    /// - Will fail if hook script cannot be read or executed for whatever reason.
    /// - Will fail if working directory path cannot be properly expanded.
    pub fn run(
        &self,
        cmd: impl AsRef<str>,
        kind: HookKind,
        repos: Option<&Vec<String>>,
    ) -> Result<()> {
        if self.action == HookAction::Never {
            return Ok(());
        }

        if self.entries.hooks.is_none() {
            return Ok(());
        }

        if let Some(hooks) = self.entries.hooks.as_ref().unwrap().get(cmd.as_ref()) {
            for hook in hooks {
                let name = match kind {
                    HookKind::Pre => hook.pre.as_ref(),
                    HookKind::Post => hook.post.as_ref(),
                };
                let name = match name {
                    Some(name) => name,
                    None => continue,
                };

                if let Some(repos) = repos {
                    if let Some(repo) = &hook.repo {
                        if !repos.contains(repo) {
                            continue;
                        }
                    }
                } else if hook.repo.is_some() {
                    warn!(
                        "Command {:?} cannot operate on targets, skipping {hook:?}",
                        cmd.as_ref()
                    );
                    continue;
                }

                let path = config_dir()?.join("hooks").join(name);
                let data = read_to_string(&path).with_context(|| "Script {path:?} undefined")?;
                let work_dir = if let Some(work_dir) = &hook.work_dir {
                    let path: PathBuf =
                        shellexpand::full(work_dir.to_string_lossy().as_ref())?.into_owned().into();
                    if !path.exists() {
                        warn!("Work directory {path:?} does not exist, skipping {hook:?}");
                        continue;
                    }
                    Some(path)
                } else {
                    None
                };

                if self.action == HookAction::Prompt {
                    self.pager.page_and_prompt(&path, &work_dir, &data)?;
                    if !self.pager.choice() {
                        continue;
                    }
                }

                let mut opts = ScriptOptions::new();
                opts.working_directory = work_dir;
                let (code, out, err) = run_script!(data, opts)?;
                info!("[{code}] {name:?}\nstdout: {out}\nstderr: {err}");
            }
        }

        Ok(())
    }
}

/// Command hook representation.
#[derive(Debug, Deserialize)]
pub struct CommandHooks {
    hooks: Option<HashMap<String, Vec<HookEntry>>>,
}

#[derive(Debug, Deserialize)]
pub struct HookEntry {
    /// Script to execute _before_ command.
    pub pre: Option<String>,

    /// Script to execute _after_ command.
    pub post: Option<String>,

    /// Execute script at target working directory.
    pub work_dir: Option<PathBuf>,

    /// Only execute for target repository in cluster.
    pub repo: Option<String>,
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

/// Hook variations.
#[derive(Debug, Default, PartialEq, Eq)]
pub enum HookKind {
    /// Execute _before_ command.
    #[default]
    Pre,

    /// Execute _after_ command.
    Post,
}

#[derive(Debug, Default)]
pub(crate) struct HookPager {
    choice: Arc<AtomicBool>,
}

impl HookPager {
    /// Construct new empty static pager.
    pub(crate) fn new() -> Self {
        HookPager::default()
    }

    /// Get choice of the user.
    pub(crate) fn choice(&self) -> bool {
        self.choice.load(Ordering::Relaxed)
    }

    /// Page hook script and prompt about its execution.
    ///
    /// Modifies keybindings to have "a" represent an accept response, and the "d" represent a deny
    /// response.
    ///
    /// # Errors
    ///
    /// Return errors issued by pager.
    pub(crate) fn page_and_prompt(
        &self,
        name: impl AsRef<Path>,
        work_dir: &Option<PathBuf>,
        data: impl AsRef<str>,
    ) -> Result<()> {
        let pager = Pager::new();
        let work_dir = match work_dir {
            Some(path) => path.clone(),
            None => PathBuf::from("./"),
        };

        pager.set_prompt(format!("Run {:?} at {:?}? [A]ccept/[D]eny", name.as_ref(), work_dir,))?;
        pager.show_prompt(true)?;
        pager.set_run_no_overflow(true)?;
        pager.set_line_numbers(LineNumbers::Enabled)?;
        pager.push_str(data.as_ref())?;
        pager.set_input_classifier(self.generate_key_bindings())?;
        pager.set_exit_strategy(ExitStrategy::PagerQuit)?;
        page_all(pager)?;

        Ok(())
    }

    fn generate_key_bindings(&self) -> Box<HashedEventRegister<RandomState>> {
        let mut input = HashedEventRegister::default();

        let response = self.choice.clone();
        input.add_key_events(&["a"], move |_, _| {
            response.store(true, Ordering::Relaxed);
            InputEvent::Exit
        });

        let response = self.choice.clone();
        input.add_key_events(&["d"], move |_, _| {
            response.store(false, Ordering::Relaxed);
            InputEvent::Exit
        });

        Box::new(input)
    }
}

/// Get absolute path to user's home directory.
///
/// # Errors
///
/// - Will fail if user's home directory cannot be determined.
pub fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or(anyhow!("Cannot determine path to home directory"))
}

/// Get absolute path to OCD's standard configuration directory.
///
/// # Invariants
///
/// - OCD's standard configuration directory is always relative to user's home directory.
///
/// # Errors
///
/// - Will fail if user's home directory cannot be determined.
pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|path| path.join("ocd"))
        .ok_or(anyhow!("Cannot determine path to configuration directory"))
}

/// Get absolute path to OCD's data directory.
///
/// # Invariants
///
/// - OCD's standard data directory is always relative to user's home directory.
///
/// # Errors
///
/// - Will fail if user's home directory cannot be determined.
pub fn data_dir() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|path| path.join("ocd"))
        .ok_or(anyhow!("Cannot determine path to data directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Context;
    use pretty_assertions::assert_eq as pretty_assert_eq;
    use sealed_test::prelude::*;
    use simple_test_case::test_case;

    #[test_case(
        r#"
            [settings]
            work_dir_alias = "home_dir"
        "#,
        RootEntry {
            settings: RootEntrySettings {
                work_dir_alias: WorkDirAlias::new("some/path"),
                excluded: None,
            }
        };
        "home_dir"
    )]
    #[test_case(
        r#"
            [settings]
            work_dir_alias = "config_dir"
        "#,
        RootEntry {
            settings: RootEntrySettings {
                work_dir_alias: WorkDirAlias::new("some/path/.config/ocd"),
                excluded: None,
            }
        };
        "config_dir"
    )]
    #[sealed_test(env = [("HOME", "some/path"), ("XDG_CONFIG_HOME", "some/path/.config")])]
    fn root_entry_valid_work_dir_alias(config: &str, expect: RootEntry) -> Result<()> {
        let result: RootEntry = toml::de::from_str(config)?;
        pretty_assert_eq!(result, expect);
        Ok(())
    }

    #[test]
    fn root_entry_invalid_work_dir_alias() {
        let config = r#"
            [settings]
            work_dir_alias = "data_dir"
        "#;
        let result: Result<RootEntry> = toml::de::from_str(config).with_context(|| "should fail!");
        assert!(result.is_err());
    }

    #[test_case(
        r#"
            [settings]
            deployment = "normal"
            url = "https://some/url"
        "#,
        NodeEntry  {
            settings: NodeEntrySettings {
                deployment: NodeEntryDeployment {
                    kind: DeploymentKind::Normal,
                    work_dir_alias: WorkDirAlias::try_default()?,
                },
                url: "https://some/url".into(),
                excluded: None,
                dependencies: None,
            }
        };
        "str_normal"
    )]
    #[test_case(
        r#"
            [settings]
            deployment = "bare_alias"
            url = "https://some/url"
        "#,
        NodeEntry  {
            settings: NodeEntrySettings {
                deployment: NodeEntryDeployment {
                    kind: DeploymentKind::BareAlias,
                    work_dir_alias: WorkDirAlias::new("some/path"),
                },
                url: "https://some/url".into(),
                excluded: None,
                dependencies: None,
            }
        };
        "str_bare_alias"
    )]
    #[test_case(
        r#"
            [settings]
            deployment = { kind = "normal", work_dir_alias = "blah/blah" }
            url = "https://some/url"
        "#,
        NodeEntry  {
            settings: NodeEntrySettings {
                deployment: NodeEntryDeployment {
                    kind: DeploymentKind::Normal,
                    work_dir_alias: WorkDirAlias::new("blah/blah"),
                },
                url: "https://some/url".into(),
                excluded: None,
                dependencies: None,
            }
        };
        "map_normal"
    )]
    #[test_case(
        r#"
            [settings]
            deployment = { kind = "bare_alias", work_dir_alias = "blah/blah" }
            url = "https://some/url"
        "#,
        NodeEntry  {
            settings: NodeEntrySettings {
                deployment: NodeEntryDeployment {
                    kind: DeploymentKind::BareAlias,
                    work_dir_alias: WorkDirAlias::new("blah/blah"),
                },
                url: "https://some/url".into(),
                excluded: None,
                dependencies: None,
            }
        };
        "map_bare_alias"
    )]
    #[sealed_test(env = [("HOME", "some/path"), ("XDG_CONFIG_HOME", "some/path/.config")])]
    fn node_entry_valid_deployment(config: &str, expect: NodeEntry) -> Result<()> {
        let node: NodeEntry = toml::de::from_str(config)?;
        pretty_assert_eq!(node, expect);
        Ok(())
    }

    #[test_case(
        r#"
            [settings]
            deployment = "snafu"
            url = "https://some/url"
        "#;
        "invalid_str"
    )]
    #[test_case(
        r#"
            [settings]
            deployment = { kind = "snafu", work_dir_alias = "blah/blah" }
            url = "https://some/url"
        "#;
        "unknown_field"
    )]
    fn node_entry_invalid_deployment(config: &str) {
        let result: Result<NodeEntry> = toml::de::from_str(config).with_context(|| "should fail!");
        assert!(result.is_err());
    }
}
