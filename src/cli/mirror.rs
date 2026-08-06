//! `baudelaire mirror`: vendor the Typst packages a site imports.

use std::path::PathBuf;

use clap::Args;

use super::{Cx, Run, group, help};
use crate::config::Config;
use crate::error::Result;
use crate::error::warning::MirrorDefaults;
use crate::mirror::Mirror;

/// Arguments for `baudelaire mirror`.
#[derive(Args, Debug, Clone)]
#[command(after_help = MirrorArgs::help())]
pub struct MirrorArgs {
    /// Write the typst packages into typst's own package directory, shared with
    /// every other project, instead of into this one.
    #[arg(long, help_heading = group::TARGETS)]
    pub global: bool,
    /// Write the typst packages here instead, wherever that is.
    #[arg(long, value_name = "DIR", conflicts_with = "global", help_heading = group::TARGETS)]
    pub path: Option<PathBuf>,
    /// Remove what a previous run wrote, instead of writing.
    #[arg(long, help_heading = group::TARGETS)]
    pub uninstall: bool,
}

impl MirrorArgs {
    /// Appended to `mirror --help`: the command exists for one reason, and a
    /// reader who does not know that reason cannot guess it from the name.
    fn help() -> String {
        format!(
            "{}\n{}",
            help::About(
                "The `@baudelaire/*` typst modules a template imports and the\n\
                 `baudelaire:*` modules a bundled script imports are generated during\n\
                 a build, so an editor has nothing on disk to resolve and marks every\n\
                 import unknown. This writes both out where an editor finds them: the\n\
                 typst modules as ordinary packages, the JavaScript ones as one\n\
                 TypeScript declaration file. Both land in the project, since three\n\
                 of the four typst modules describe *this* site; `--global` shares\n\
                 one copy of them between every project instead.\n\
                 \n\
                 A build never reads what this writes, so a stale copy cannot change a\n\
                 page. Every build refreshes the declarations; re-run this after\n\
                 upgrading baudelaire.\n\
                 \n\
                 The run ends in the one setting each family needs, so an editor\n\
                 resolves it. `-v` lists every module it wrote."
            ),
            help::Table::examples(&[
                ("baudelaire mirror", "Into .baudelaire/generated"),
                (
                    "baudelaire mirror --global",
                    "Typst packages into typst's own directory, shared"
                ),
                (
                    "baudelaire mirror --path .typst",
                    "Typst packages into a directory of your own"
                ),
                ("baudelaire mirror --uninstall", "Take it all back off"),
            ])
        )
    }

    /// The project whose data the modules are generated from, announced under
    /// the verb of the run.
    ///
    /// Optional, and deliberately: `html` and `site` are worth mirroring from
    /// anywhere (after an upgrade, say, with no project in sight), and the two
    /// table modules mirror empty outside a project exactly as they do inside
    /// one that has never been built.
    ///
    /// A config that *exists* and does not parse is the case worth reporting,
    /// exactly as `clean` reports it: the diagnostic used to be discarded
    /// outright, so the `site` module an editor resolves against carried the
    /// default title and url with no word to the reader, while `baudelaire
    /// build` failed on the same project. No project at all stays silent, since
    /// there is nothing there to have got wrong.
    fn config(&self, cx: &Cx) -> Config {
        let verb = match self.uninstall {
            true => "removing modules for",
            false => "mirroring modules for",
        };
        match cx.announced(verb) {
            Ok(config) => config,
            Err(error) => {
                cx.ui.banner(verb.trim_end_matches(" for"));
                if cx.cli.global.config.exists() {
                    cx.ui.warn(MirrorDefaults {
                        errors: vec![error],
                    });
                }
                Config {
                    root: cx.root.path().to_path_buf(),
                    ..Config::default()
                }
            }
        }
    }
}

impl Run for MirrorArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = self.config(cx);
        let mirror = Mirror::new(&config, self.path.as_deref(), self.global);
        if self.uninstall {
            // The inverse of mirroring belongs to the command that mirrors, not
            // to `clean`: an install is machine-global state that no config
            // locates, and `--path` means only the run that wrote it knows
            // where it went. `clean` stays what it says it is, project state.
            mirror.uninstall()?.render(cx.ui);
            return Ok(());
        }
        let settings = mirror.install()?.render(cx.ui);
        settings.render(cx.ui);
        Ok(())
    }
}
