//! External command hooks: the escape hatch for tools baudelaire does not
//! embed (Tailwind, PostCSS, Pagefind, deploy scripts). `before` hooks run
//! ahead of the asset pipeline so anything they generate into `assets/` is
//! fingerprinted like a first-class asset; `after` hooks run once `dist` is
//! written.
//!
//! Commands run through the system shell in the project's working directory,
//! inheriting stdio so their output streams straight to the terminal.

use std::process::Command;

use tracing::debug;

use crate::config::Config;
use crate::error::{HookError, HookPhase, Result};
use crate::ui::Ui;

/// Runs the configured lifecycle hooks.
pub(super) struct Hooks<'a> {
    config: &'a Config,
}

impl<'a> Hooks<'a> {
    pub(super) fn new(config: &'a Config) -> Self {
        Self { config }
    }

    /// Commands run before the build.
    pub(super) fn before(&self, ui: &Ui) -> Result<()> {
        self.run(&self.config.hooks.before, HookPhase::Before, ui)
    }

    /// Commands run after the site is written.
    pub(super) fn after(&self, ui: &Ui) -> Result<()> {
        self.run(&self.config.hooks.after, HookPhase::After, ui)
    }

    fn run(&self, commands: &[String], phase: HookPhase, ui: &Ui) -> Result<()> {
        if commands.is_empty() {
            return Ok(());
        }
        // hooks run in the project root. if it can't be resolved (e.g. removed
        // underneath us), that's a typed error, not a silent empty-path fallback
        // that makes the shell fail with a baffling message.
        let cwd = std::env::current_dir().map_err(|e| HookError::cwd(phase, e))?;
        for command in commands {
            // the hook's own stdio streams straight to the terminal, so give it
            // a dimmed context line first.
            ui.detail(format_args!("$ {command}"));
            debug!(%phase, command, "running hook");
            let status = Self::shell()
                .current_dir(&cwd)
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
        if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.arg("/C");
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c");
            c
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Level;

    /// A removed working directory is a typed hook error, not a spawn in `""`.
    /// Safe to chdir: nextest runs each test in its own process.
    #[test]
    fn missing_working_directory_is_a_typed_error() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        std::fs::remove_dir_all(tmp.path()).unwrap();

        let mut config = Config::default();
        config.hooks.before = vec!["true".into()];
        let ui = Ui::new(Level::Silent);
        let err = Hooks::new(&config).before(&ui).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("working directory"), "{msg}");
        assert!(msg.contains("before-build"), "{msg}");
    }
}
