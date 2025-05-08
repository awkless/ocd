// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Data model types.
//!
//! Contains various types that represent, and help manipulate OCD's data model. Currently, the
//! [`Cluster`] type is provided as a format preserving cluster definition parser.

use crate::{
    cmd::HookAction,
    fs::{config_dir, home_dir},
    Error, Result,
};

use minus::{
    input::{HashedEventRegister, InputEvent},
    page_all, ExitStrategy, LineNumbers, Pager,
};
use run_script::{run_script, ScriptOptions};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsString,
    fs::read_to_string,
    hash::RandomState,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Key, Table, Value};
use tracing::{debug, info, instrument, warn};

/// Format preserving cluster definition parser.
///
/// Obtains valid parsing of user's cluster definition in deserialized form. Provides additional
/// utilities to make it easer to extract and serialize cluster data for further manipulation. This
/// type only operates on strings. Caller is responsible for file I/O.
///
/// # Invariants
///
/// - Node dependencies exist in cluster.
/// - Node dependencies are acyclic.
/// - Directory aliases are always expanded.
#[derive(Clone, Default, Debug)]
pub struct Cluster {
    /// Root of cluster definition.
    pub root: RootEntry,

    /// All node entries in cluster definition represented as DAG.
    pub nodes: HashMap<String, NodeEntry>,

    document: DocumentMut,
}

impl Cluster {
    /// Construct new empty cluster definition.
    pub fn new() -> Self {
        Cluster::default()
    }

    /// Get node entry from cluster definition.
    ///
    /// # Errors
    ///
    /// - Return [`Error::EntryNotFound`] if node does not exist in cluster definition.
    pub fn get_node(&self, name: impl AsRef<str>) -> Result<&NodeEntry> {
        self.nodes.get(name.as_ref()).ok_or(Error::EntryNotFound { name: name.as_ref().into() })
    }

    /// Add node entry into cluster definition.
    ///
    /// Will replace existing node entry if given node entry was not new, returning the old entry
    /// that was replaced. Will construct a new "nodes" table if it does not already exist.
    ///
    /// # Errors
    ///
    /// - Return [`Error::EntryNotTable`] if "nodes" was defined, but not as a table as expected.
    ///
    /// [`Error::EntryNotTable`]: crate::Error::EntryNotTable
    pub fn add_node(
        &mut self,
        name: impl AsRef<str>,
        node: NodeEntry,
    ) -> Result<Option<NodeEntry>> {
        let (key, item) = node.to_toml(name.as_ref());
        let table = if let Some(item) = self.document.get_mut("nodes") {
            item.as_table_mut().ok_or(Error::EntryNotTable { name: "nodes".into() })?
        } else {
            // INVARIANT: Construct new "nodes" table to insert node entry into.
            let mut new_table = Table::new();
            new_table.set_implicit(true);
            self.document.insert("nodes", Item::Table(new_table));
            // Will not panic since we just inserted the "nodes" table beforehand.
            self.document["nodes"].as_table_mut().unwrap()
        };

        // TODO: Transfer comments and whitespace of old entry to new entry that replaced it.
        //   - Is this really worth doing?
        table.insert_formatted(&key, item);

        Ok(self.nodes.insert(name.as_ref().into(), node))
    }

    /// Remove existing node entry from cluster definition.
    ///
    /// # Errors
    ///
    /// - Return [`Error::EntryNotFound`] if "nodes" table or node entry itself cannot be found.
    ///
    /// [`Error::EntryNotFound`]: crate::Error::EntryNotFound
    pub fn remove_node(&mut self, node: impl AsRef<str>) -> Result<NodeEntry> {
        self.document
            .get_mut("nodes")
            .and_then(|item| item.as_table_mut())
            .ok_or(Error::EntryNotFound { name: "nodes".into() })?
            .remove(node.as_ref())
            .ok_or(Error::EntryNotFound { name: node.as_ref().into() })?;

        self.nodes.remove(node.as_ref()).ok_or(Error::EntryNotFound { name: node.as_ref().into() })
    }

    /// Iterate through all dependencies of a target node entry.
    ///
    /// Provides full path through each dependency of target node inclusively.
    pub fn dependency_iter(&self, node: impl Into<String>) -> DependencyIter<'_> {
        let mut stack = VecDeque::new();
        stack.push_front(node.into());
        DependencyIter { graph: &self.nodes, visited: HashSet::new(), stack }
    }

    fn dependency_existence_check(&self) -> Result<()> {
        let mut results = Vec::new();
        for node in self.nodes.values() {
            for dependency in node.dependencies.iter().flatten() {
                if !self.nodes.contains_key(dependency) {
                    results.push(Err(Error::DependencyNotFound { name: dependency.clone() }));
                } else {
                    results.push(Ok(()));
                }
            }
        }

        results.into_iter().collect::<_>()
    }

    #[instrument(skip(self), level = "debug")]
    fn acyclic_check(&self) -> Result<()> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        // INVARIANT: The in-degree of each node is the sum of all incoming edges to each
        // destination node.
        for (name, node) in &self.nodes {
            in_degree.entry(name.clone()).or_insert(0);
            for dependency in node.dependencies.iter().flatten() {
                *in_degree.entry(dependency.clone()).or_insert(0) += 1;
            }
        }

        // INVARIANT: Queue always contains nodes with in-degree of 0, i.e., nodes with no incoming
        // edges.
        for (name, degree) in &in_degree {
            if *degree == 0 {
                queue.push_back(name.clone());
            }
        }

        // BFS traversal such that the in-degree of all dependencies of a popped node from queue is
        // decremented by one. If a given dependency's in-degree becomes zero, push it into the
        // queue to be traversed. Finally, mark the currently popped node as visisted.
        while let Some(current) = queue.pop_front() {
            for dependency in self.nodes[&current].dependencies.iter().flatten() {
                *in_degree.get_mut(dependency).unwrap() -= 1;
                if *in_degree.get(dependency).unwrap() == 0 {
                    queue.push_back(dependency.clone());
                }
            }
            // INVARIANT: Visited nodes represent the topological sort of graph.
            visited.insert(current);
        }

        // INVARIANT: Queue is empty, but graph has not been fully visited.
        //   - There exists a cycle.
        //   - The unvisited nodes represent this cycle.
        if visited.len() != self.nodes.len() {
            let cycle: Vec<String> =
                self.nodes.keys().filter(|key| !visited.contains(*key)).cloned().collect();

            // TODO: Pretty print structure of cycle, besides printing names of problematic nodes.
            return Err(Error::CircularDependencies { cycle });
        }

        debug!("Topological sort of cluster nodes: {visited:?}");

        Ok(())
    }

    fn expand_dir_aliases(&mut self) -> Result<()> {
        for node in self.nodes.values_mut() {
            if let DeploymentKind::BareAlias(dir_alias) = &node.deployment {
                node.deployment = DeploymentKind::BareAlias(DirAlias::new(
                    shellexpand::full(&dir_alias.to_string())?.into_owned(),
                ));
            }
        }

        Ok(())
    }
}

impl std::fmt::Display for Cluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.document)
    }
}

impl std::str::FromStr for Cluster {
    type Err = Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let document: DocumentMut = data.parse()?;
        let root = RootEntry::try_from(document.as_table())?;
        let nodes = if let Some(entries) = document.get("nodes").and_then(|node| node.as_table()) {
            let mut nodes: HashMap<String, NodeEntry> = HashMap::new();
            for (key, value) in entries.iter() {
                nodes.insert(key.into(), NodeEntry::try_from(value)?);
            }
            nodes
        } else {
            HashMap::new()
        };

        let mut cluster = Self { root, nodes, document };
        cluster.dependency_existence_check()?;
        cluster.acyclic_check()?;
        cluster.expand_dir_aliases()?;

        Ok(cluster)
    }
}

/// Iterator for generating valid node entry dependency paths.
///
/// # Invariants
///
/// Nodes and their dependencies are acyclic.
#[derive(Debug)]
pub struct DependencyIter<'cluster> {
    graph: &'cluster HashMap<String, NodeEntry>,
    visited: HashSet<String>,
    stack: VecDeque<String>,
}

impl<'cluster> Iterator for DependencyIter<'cluster> {
    type Item = (&'cluster str, &'cluster NodeEntry);

    fn next(&mut self) -> Option<Self::Item> {
        // INVARIANT: Nodes and their dependencies are acyclic through acyclic check performed
        // during deserialization through `Cluster::from_str`.
        if let Some(node) = self.stack.pop_front() {
            let (name, node) = self.graph.get_key_value(&node)?;
            for dependency in node.dependencies.iter().flatten() {
                if !self.visited.contains(dependency) {
                    self.stack.push_front(dependency.clone());
                    self.visited.insert(dependency.clone());
                }
            }

            return Some((name.as_ref(), node));
        }

        None
    }
}

/// Root entry of cluster definition.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct RootEntry {
    /// Target directory to act as worktree alias for deployment.
    pub dir_alias: DirAlias,

    /// List of sparsity rules to exclude files from deployment.
    pub excluded: Option<Vec<String>>,
}

impl RootEntry {
    /// Construct new empty root entry.
    pub fn new() -> Self {
        RootEntry::default()
    }
}

impl<'toml> TryFrom<&'toml Table> for RootEntry {
    type Error = Error;

    /// Try to deserialize TOML table to [`RootEntry`].
    ///
    /// If field `dir_alias` is not defined, then it will default to using OCD's configuration
    /// directory path.
    ///
    /// # Errors
    ///
    /// - Return [`Error::NoWayConfig`] if OCD's configuration directory path could not be
    ///   determined.
    ///
    /// [`Error::NoWayConfig`]: crate::Error::NoWayConfig
    fn try_from(table: &'toml Table) -> Result<Self, Self::Error> {
        let mut root = RootEntry::new();

        let dir_alias = if let Some(entry) = table.get("dir_alias") {
            if let Some(alias) = entry.as_str() {
                if alias == "config_dir" {
                    config_dir()?
                } else if alias == "home_dir" {
                    home_dir()?
                } else {
                    warn!("Invalid value {alias:?} for \"root.dir_alias\", using default");
                    config_dir()?
                }
            } else {
                config_dir()?
            }
        } else {
            config_dir()?
        };
        root.dir_alias = DirAlias::new(dir_alias);

        root.excluded = table.get("excluded").and_then(|rules| {
            rules.as_array().map(|arr| {
                arr.into_iter().map(|rule| rule.as_str().unwrap_or_default().into()).collect()
            })
        });

        Ok(root)
    }
}

/// Node entry for cluster configuration.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct NodeEntry {
    /// Method of deployment for node entry.
    pub deployment: DeploymentKind,

    /// URL to clone node entry from.
    pub url: String,

    /// List of sparsity rules to exclude files from deployment.
    pub excluded: Option<Vec<String>>,

    /// List of node dependencies to include for deployment.
    pub dependencies: Option<Vec<String>>,
}

impl NodeEntry {
    /// Construct new empty node entry.
    pub fn new() -> Self {
        NodeEntry::default()
    }

    pub fn to_toml(&self, name: impl AsRef<str>) -> (Key, Item) {
        let mut node = Table::new();

        match &self.deployment {
            DeploymentKind::Normal => {
                node.insert("deployment", Item::Value(Value::from("normal")));
            }
            DeploymentKind::BareAlias(alias) => {
                if alias.is_home_dir() {
                    node.insert("deployment", Item::Value(Value::from("bare_alias")));
                } else {
                    let mut inline = InlineTable::new();
                    inline.insert("kind", Value::from("bare_alias"));
                    inline.insert("dir_alias", Value::from(alias.to_string()));
                    node.insert("deployment", Item::Value(Value::from(inline)));
                }
            }
        }

        node.insert("url", Item::Value(Value::from(&self.url)));

        if let Some(excluded) = &self.excluded {
            node.insert("excluded", Item::Value(Value::Array(Array::from_iter(excluded))));
        }

        if let Some(dependencies) = &self.dependencies {
            node.insert("dependencies", Item::Value(Value::Array(Array::from_iter(dependencies))));
        }

        let key = Key::new(name.as_ref());
        let value = Item::Table(node);
        (key, value)
    }
}

impl<'toml> TryFrom<&'toml Item> for NodeEntry {
    type Error = Error;

    fn try_from(item: &'toml Item) -> Result<Self, Self::Error> {
        let mut node = NodeEntry::new();

        node.deployment = if let Some(deployment) = item.get("deployment") {
            if let Some(entry) = deployment.as_str() {
                match entry {
                    "normal" => DeploymentKind::Normal,
                    "bare_alias" => DeploymentKind::BareAlias(DirAlias::new(home_dir()?)),
                    &_ => DeploymentKind::default(),
                }
            } else {
                let kind =
                    deployment.get("kind").and_then(|kind| kind.as_str()).unwrap_or_default();
                let alias = deployment
                    .get("dir_alias")
                    .and_then(|alias| alias.as_str().map(Into::into))
                    .unwrap_or(home_dir()?);
                match kind {
                    "normal" => DeploymentKind::Normal,
                    "bare_alias" => DeploymentKind::BareAlias(DirAlias::new(alias)),
                    &_ => DeploymentKind::default(),
                }
            }
        } else {
            DeploymentKind::default()
        };

        node.url = item.get("url").and_then(|url| url.as_str().map(Into::into)).unwrap_or_default();

        node.excluded = item.get("excluded").and_then(|rules| {
            rules.as_array().map(|arr| {
                arr.into_iter().map(|rule| rule.as_str().unwrap_or_default().into()).collect()
            })
        });

        node.dependencies = item.get("dependencies").and_then(|deps| {
            deps.as_array().map(|arr| {
                arr.into_iter().map(|dep| dep.as_str().unwrap_or_default().into()).collect()
            })
        });

        Ok(node)
    }
}

/// The variants of node deployment.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum DeploymentKind {
    /// Just make sure node entry is cloned.
    #[default]
    Normal,

    /// Make sure node entry is cloned, and deployed or undeployed to directory alias.
    BareAlias(DirAlias),
}

impl DeploymentKind {
    /// Determine if deployment kind yields a bare repository.
    pub(crate) fn is_bare(&self) -> bool {
        match self {
            DeploymentKind::Normal => false,
            DeploymentKind::BareAlias(..) => true,
        }
    }
}

/// Directory path to use as an alias for a worktree.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct DirAlias(pub(crate) PathBuf);

impl DirAlias {
    /// Construct new directory alias from given path.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Determine if directory alias if pointing to home directory path.
    pub(crate) fn is_home_dir(&self) -> bool {
        let home = match home_dir() {
            Ok(path) => path,
            Err(_) => return false,
        };

        if self.0 == home {
            return true;
        }

        false
    }

    pub(crate) fn to_os_string(&self) -> OsString {
        OsString::from(self.0.to_string_lossy().into_owned())
    }
}

impl std::fmt::Display for DirAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string_lossy().into_owned())
    }
}

/// Execute user-defined hooks.
///
/// Invariants:
///
/// - Always expands working directory paths.
#[derive(Debug, Default)]
pub struct HookRunner {
    entries: CommandHooks,
    action: HookAction,
    pager: HookPager,
}

impl HookRunner {
    /// Set hook action type.
    pub fn set_action(&mut self, action: HookAction) {
        self.action = action;
    }

    /// Run all hooks targeting a specific command.
    ///
    /// Skips empty hook entry listing, or hooks that match a specific target. Issues pager to both
    /// show what the contents of a given hook looks like, and prompts about its execution when
    /// using the prompt hook action. Otherwise, it will either execute hooks with no prompting, or
    /// not execute any hooks based on hook action.
    ///
    /// # Errors
    ///
    /// - Return [`Error::Minus`] for any pager failures.
    /// - Return [`Error::RunScript`] for any failure to execute hook script.
    /// - Return [`Error::Shellexpand`] for failure to expand workdir.
    pub fn run(
        &self,
        cmd: impl AsRef<str>,
        kind: HookKind,
        targets: Option<&Vec<String>>,
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

                if let Some(targets) = targets {
                    if let Some(target) = &hook.node {
                        if !targets.contains(target) {
                            continue;
                        }
                    }
                } else if hook.node.is_some() {
                    warn!(
                        "Command {:?} cannot operate on targets, skipping {hook:?}",
                        cmd.as_ref()
                    );
                    continue;
                }

                let path = config_dir()?.join("hooks").join(name);
                let data = read_to_string(&path)?;
                let workdir = if let Some(workdir) = &hook.workdir {
                    // INVARIANT: Always expand work directory.
                    //   - Skip if work directory does not exist.
                    let path: PathBuf =
                        shellexpand::full(workdir.to_string_lossy().as_ref())?.into_owned().into();
                    if !path.exists() {
                        warn!("Workdir {path:?} does not exist, skipping {hook:?}");
                        continue;
                    }
                    Some(path)
                } else {
                    None
                };

                if self.action == HookAction::Prompt {
                    self.pager.page_and_prompt(&path, &workdir, &data)?;
                    if !self.pager.choice() {
                        continue;
                    }
                }

                let mut opts = ScriptOptions::new();
                opts.working_directory = workdir;
                let (code, out, err) = run_script!(data, opts)?;
                info!("[{code}] {name:?}\nstdout: {out}\nstderr: {err}");
            }
        }

        Ok(())
    }
}

impl std::str::FromStr for HookRunner {
    type Err = Error;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let entries: CommandHooks = toml_edit::de::from_str(data)?;
        Ok(Self { entries, ..Default::default() })
    }
}

/// Command hook representation.
///
/// No format preservation, because OCD is never expected to modify the user's hook configuration
/// file.
#[derive(Clone, Debug, Deserialize, Default, Eq, PartialEq)]
pub struct CommandHooks {
    pub hooks: Option<HashMap<String, Vec<HookEntry>>>,
}

/// Hook entry for command hook.
#[derive(Clone, Debug, Deserialize, Default, Eq, PartialEq)]
pub struct HookEntry {
    /// Execute _before_ command.
    pub pre: Option<String>,

    /// Execute _after_ command.
    pub post: Option<String>,

    /// Execute at target working directory.
    pub workdir: Option<PathBuf>,

    /// Only execute for target node.
    pub node: Option<String>,
}

/// Hook variations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum HookKind {
    /// Execute _before_ command.
    #[default]
    Pre,

    /// Execute _after_ command.
    Post,
}

/// Use Minus pager to show and prompt hook entry.
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
    /// # Errors
    ///
    /// - Return [`Error::Minus`] for any pager failures.
    pub(crate) fn page_and_prompt(
        &self,
        filename: impl AsRef<Path>,
        workdir: &Option<PathBuf>,
        data: impl AsRef<str>,
    ) -> Result<()> {
        let pager = Pager::new();
        let workdir = match workdir {
            Some(path) => path.clone(),
            None => PathBuf::from("./"),
        };

        pager.set_prompt(format!(
            "Run {:?} at {:?}? [a]ccept/[d]eny",
            filename.as_ref(),
            workdir
        ))?;
        pager.show_prompt(true)?;
        pager.set_run_no_overflow(true)?;
        pager.set_line_numbers(LineNumbers::Enabled)?;
        pager.push_str(data.as_ref())?;
        pager.set_input_classifier(self.generate_key_bindings())?;
        pager.set_exit_strategy(ExitStrategy::PagerQuit)?;
        page_all(pager)?;

        Ok(())
    }

    /// Set default keybindings for pager.
    ///
    /// Using "a" key for accepting the hook, and the "d" key for rejecting the hook from
    /// execution.
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
