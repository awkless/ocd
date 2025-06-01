// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! Hook configuration parser.
//!
//! Provides methods to parse, deserialize, and execute command hooks.

use super::config_dir;

use anyhow::{Context, Result};
use clap::ValueEnum;
use config::{Config, File};
use minus::{
    input::{HashedEventRegister, InputEvent},
    page_all, ExitStrategy, LineNumbers, Pager,
};
use run_script::{run_script, ScriptOptions};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs::read_to_string,
    hash::RandomState,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
};
use tracing::{debug, info, trace, warn};

/// Execute user defined command hooks.
///
/// Command hooks are defined at `$XDG_CONFIG_HOME/ocd/hooks.toml` such that all hook scripts used
/// for a hook entry are stored at `$XDG_CONFIG_HOME/ocd/hooks/`. This type will not look anywhere
/// else for hook scripts.
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
                    if let Some(repo) = &hook.target {
                        if !repos.contains(repo) {
                            continue;
                        }
                    }
                } else if hook.target.is_some() {
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
    pub target: Option<String>,
}

/// Behavior variants for hook execution.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum HookAction {
    /// Always execute hooks no questions asked.
    Always,

    /// Prompt user with hook's contents through pager.
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
