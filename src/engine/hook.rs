//! External command hooks: the escape hatch for tools baudelaire does not
//! embed (Tailwind, PostCSS, Pagefind, deploy scripts). `before` hooks run
//! ahead of the asset pipeline so anything they generate into `assets/` is
//! fingerprinted like a first-class asset; `after` hooks run once `dist` is
//! written.
//!
//! Commands run through the system shell in the project's working directory,
//! inheriting stdio so their output streams straight to the terminal.

use std::process::Command;

use crate::cli::output::Report;
use crate::config::Config;
use crate::error::{HookError, HookPhase, Result};

/// Runs the configured lifecycle hooks.
pub(super) struct Hooks<'a> {
    config: &'a Config,
}

impl<'a> Hooks<'a> {
    pub(super) fn new(config: &'a Config) -> Self {
        Self { config }
    }

    /// Commands run before the build.
    pub(super) fn before(&self, report: &mut Report) -> Result<()> {
        self.run(&self.config.hooks.before, HookPhase::Before, report)
    }

    /// Commands run after the site is written.
    pub(super) fn after(&self, report: &mut Report) -> Result<()> {
        self.run(&self.config.hooks.after, HookPhase::After, report)
    }

    fn run(&self, commands: &[String], phase: HookPhase, report: &mut Report) -> Result<()> {
        for command in commands {
            report.info(format_args!("hook: {command}"))?;
            let status = Self::shell()
                .arg(command)
                .status()
                .map_err(|e| HookError::spawn(phase, command, e))?;
            if !status.success() {
                return Err(HookError::failed(phase, command, status).into());
            }
        }
        Ok(())
    }

    /// The platform shell configured to take a command string.
    fn shell() -> Command {
        let mut command = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.arg("/C");
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c");
            c
        };
        command.current_dir(std::env::current_dir().unwrap_or_default());
        command
    }
}
