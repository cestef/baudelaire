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
        // Hooks run in the project root, which is what every other configured
        // path is relative to. Taking it from the config rather than the process
        // cwd is what makes a hook's own relative paths (`> assets/gen.css`)
        // land in the site being built, not wherever the process happened to
        // start: the CLI chdirs to the root, an embedder does not.
        let cwd = &self.config.root;
        for command in commands {
            // the hook's own stdio streams straight to the terminal, so give it
            // a dimmed context line first.
            ui.detail(format_args!("$ {command}"));
            debug!(%phase, command, "running hook");
            let status = Self::shell()
                .current_dir(cwd)
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

    /// A hook runs in the configured project root, not wherever the process
    /// happens to be, so its own relative paths land in the site being built.
    /// Safe to chdir: nextest runs each test in its own process.
    #[test]
    fn a_hook_runs_in_the_project_root_not_the_process_cwd() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::env::set_current_dir(elsewhere.path()).unwrap();

        let mut config = Config {
            root: root.path().to_path_buf(),
            ..Config::default()
        };
        config.hooks.before = vec!["printf x > marker.txt".into()];
        Hooks::new(&config).before(&Ui::new(Level::Silent)).unwrap();

        assert!(
            root.path().join("marker.txt").exists(),
            "hook did not write into the project root"
        );
        assert!(
            !elsewhere.path().join("marker.txt").exists(),
            "hook wrote into the process cwd instead"
        );
    }

    /// A root the hook cannot enter stays a typed spawn error naming the
    /// command, not a bare io error from somewhere in the shell.
    #[test]
    fn a_missing_project_root_is_a_typed_spawn_error() {
        let tmp = tempfile::tempdir().unwrap();
        let mut config = Config {
            root: tmp.path().join("gone"),
            ..Config::default()
        };
        config.hooks.before = vec!["true".into()];

        let err = Hooks::new(&config)
            .before(&Ui::new(Level::Silent))
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("could not run before-build hook"), "{msg}");
        assert!(msg.contains("`true`"), "{msg}");
    }
}
