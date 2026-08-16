//! `baudelaire clean`: remove what the build wrote or cached.

use std::path::{Path, PathBuf};

use clap::Args;

use super::{Cx, Run, group, prompt};
use crate::config::Config;
use crate::error::Result;
use crate::error::warning::{CleanDefaults, CleanRefused};
use crate::ui::Ui;

/// With no target flag every directory is swept; naming targets narrows the
/// sweep to those.
#[derive(Args, Debug, Clone, Default)]
pub struct CleanArgs {
    /// Remove everything: the output directory and all local build state.
    #[arg(long, help_heading = group::TARGETS)]
    pub all: bool,
    /// Remove the build output directory, wherever `paths { dist }` puts it.
    /// Named for what it removes rather than for the key that locates it;
    /// `--dist` still works.
    #[arg(long, alias = "dist", help_heading = group::TARGETS)]
    pub output: bool,
    /// Remove the incremental build cache.
    #[arg(long, help_heading = group::TARGETS)]
    pub cache: bool,
    /// Remove local announce state.
    #[arg(long, help_heading = group::TARGETS)]
    pub announce: bool,

    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Report what would be removed without removing it.
    #[arg(long)]
    pub dry_run: bool,
}

/// One nameable `clean` target: the flag that selects it and the directories it
/// removes.
struct CleanTarget {
    selected: fn(&CleanArgs) -> bool,
    dirs: fn(&Config) -> Vec<PathBuf>,
}

const CLEAN_TARGETS: &[CleanTarget] = &[
    CleanTarget {
        selected: |a| a.output,
        dirs: |c| vec![c.paths.dist.clone()],
    },
    CleanTarget {
        selected: |a| a.cache,
        dirs: |c| vec![c.cache.dir.clone()],
    },
    CleanTarget {
        selected: |a| a.announce,
        dirs: |_| vec![Config::scratch(crate::config::Scratch::Announce)],
    },
];
impl CleanArgs {
    /// Whether this is the wholesale wipe; naming no target is shorthand for it.
    pub(super) fn all(&self) -> bool {
        self.all || CLEAN_TARGETS.iter().all(|t| !(t.selected)(self))
    }

    /// Remove this invocation's targets, skipping any that would take the
    /// project with them. What is refused is settled before the listing, so a
    /// `--dry-run` previews exactly what a real run would take.
    fn sweep(
        &self,
        ui: &Ui,
        config: &Config,
        root: &Path,
        interaction: &dyn crate::remote::Interaction,
    ) -> Result<()> {
        let (dirs, refused): (Vec<PathBuf>, Vec<PathBuf>) = self
            .targets(config)
            .into_iter()
            .filter(|dir| dir.exists())
            .partition(|dir| Self::removable(dir, root));
        for dir in refused {
            ui.warn(CleanRefused { dir });
        }
        if dirs.is_empty() {
            ui.done("nothing to clean");
            return Ok(());
        }
        for dir in &dirs {
            ui.detail(format_args!("- {}", dir.display()));
        }
        if self.dry_run {
            ui.done(format_args!("{} to remove", Self::count(dirs.len())));
            return Ok(());
        }
        if !self.consented(interaction, dirs.len())? {
            ui.done(format_args!(
                "declined; {} left in place",
                Self::count(dirs.len())
            ));
            return Ok(());
        }
        for dir in &dirs {
            crate::fs::remove_dir_all(dir)?;
        }
        ui.done("clean");
        Ok(())
    }

    /// The config to sweep by, falling back to the built-in paths when the file
    /// exists and does not load. A missing config stays an error, or `clean`
    /// would sweep the built-in paths out of whatever directory it was run in.
    fn config(cx: &Cx) -> Result<Config> {
        match cx.announced("cleaning") {
            Ok(config) => Ok(config),
            Err(error) if cx.cli.global.config.exists() => {
                cx.ui.banner("cleaning");
                cx.ui.warn(CleanDefaults {
                    errors: vec![error],
                });
                Ok(Config {
                    root: cx.root.path().to_path_buf(),
                    ..Config::default()
                })
            }
            Err(error) => Err(error),
        }
    }

    /// Whether the sweep may go ahead. Only the wholesale wipe asks: it takes
    /// announce state with it, which decides what the next `announce` does to a
    /// live repository.
    pub(super) fn consented(
        &self,
        interaction: &dyn crate::remote::Interaction,
        count: usize,
    ) -> Result<bool> {
        use crate::remote::Consent;
        if !self.all() {
            return Ok(true);
        }
        let action = format!("remove {}", Self::count(count));
        match interaction.consent(&action, self.yes)? {
            Consent::Granted => Ok(true),
            Consent::Refused => Ok(false),
            Consent::Unattended => Err(crate::error::Unattended { action }.into()),
        }
    }

    /// `1 directory` / `3 directories`, for a prompt that reads as a sentence.
    fn count(dirs: usize) -> String {
        match dirs {
            1 => "1 directory".to_owned(),
            n => format!("{n} directories"),
        }
    }

    /// Whether `dir` may be removed: it must not be the project root, nor an
    /// ancestor of it. A `dist` placed outside the project stays cleanable.
    pub(super) fn removable(dir: &Path, root: &Path) -> bool {
        !crate::fs::canonical(root).starts_with(crate::fs::canonical(dir))
    }

    /// The directories to remove for this invocation: a full sweep clears the
    /// output plus the whole scratch root, naming a relocated cache dir
    /// separately; a narrowed one removes only the selected [`CLEAN_TARGETS`].
    pub(super) fn targets(&self, config: &Config) -> Vec<PathBuf> {
        if self.all() {
            let mut dirs = vec![config.paths.dist.clone(), PathBuf::from(Config::SCRATCH)];
            if !config.cache.dir.starts_with(Config::SCRATCH) {
                dirs.push(config.cache.dir.clone());
            }
            return dirs;
        }
        CLEAN_TARGETS
            .iter()
            .filter(|t| (t.selected)(self))
            .flat_map(|t| (t.dirs)(config))
            .collect()
    }
}

impl Run for CleanArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = Self::config(cx)?;
        self.sweep(cx.ui, &config, cx.root.path(), &prompt::Tty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Level;

    struct Declines;

    impl crate::remote::Interaction for Declines {
        fn interactive(&self) -> bool {
            true
        }
        fn confirm(&self, _prompt: &str) -> Result<bool> {
            Ok(false)
        }
        fn secret(&self, _label: &str) -> Result<Option<String>> {
            Ok(None)
        }
    }

    /// A project whose `dist` is the project root itself (the config mistake
    /// `removable` exists for) and a cache directory beside it.
    fn project() -> (tempfile::TempDir, Config) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        std::fs::create_dir_all(root.join("cache")).expect("mkdir");
        let mut config = Config {
            root: root.clone(),
            ..Config::default()
        };
        config.paths.dist = root.clone();
        config.cache.dir = root.join("cache");
        (tmp, config)
    }

    #[test]
    fn a_dry_run_leaves_out_what_a_real_run_would_refuse() {
        let (tmp, config) = project();
        let ui = Ui::new(Level::Silent);
        let args = CleanArgs {
            output: true,
            cache: true,
            dry_run: true,
            ..CleanArgs::default()
        };
        args.sweep(&ui, &config, tmp.path(), &Declines)
            .expect("a dry run removes nothing and fails at nothing");
        assert_eq!(ui.warnings(), 1);
        assert!(tmp.path().join("cache").is_dir());
    }

    #[test]
    fn a_declined_sweep_removes_nothing_and_does_not_claim_otherwise() {
        let (tmp, config) = project();
        let ui = Ui::new(Level::Silent);
        CleanArgs::default()
            .sweep(&ui, &config, tmp.path(), &Declines)
            .expect("a refusal is an answer, not a failure");
        assert!(tmp.path().join("cache").is_dir());
    }
}
