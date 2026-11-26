//! `baudelaire mirror`: write the generated modules where an editor finds them.

use clap::Args;

use super::{Cx, Run, group, help};
use crate::config::Config;
use crate::error::Result;
use crate::error::warning::MirrorDefaults;
use crate::mirror::Mirror;

#[derive(Args, Debug, Clone)]
#[command(after_help = MirrorArgs::help())]
pub struct MirrorArgs {
    /// Remove what a previous run wrote, instead of writing.
    #[arg(long, help_heading = group::TARGETS)]
    pub uninstall: bool,
}

impl MirrorArgs {
    /// Appended to `mirror --help`.
    fn help() -> String {
        format!(
            "{}\n{}",
            help::About(
                "The `/.baudelaire/site/*.typ` modules a template imports and the\n\
                 `baudelaire:*` modules a bundled script imports are generated during\n\
                 a build. Every build writes both into the project, where an editor\n\
                 finds them; this writes them without building, for a fresh checkout.\n\
                 \n\
                 A build never reads the in-memory modules back, so a stale copy cannot\n\
                 change a page.\n\
                 \n\
                 The run ends in the one setting each family needs, so an editor\n\
                 resolves it. `-v` lists every module it wrote."
            ),
            help::Table::examples(&[
                (
                    "baudelaire mirror",
                    "Into .baudelaire/site and .baudelaire/generated"
                ),
                ("baudelaire mirror --uninstall", "Take it all back off"),
            ])
        )
    }

    /// The project whose data the modules are generated from, announced under
    /// the verb of the run. A config that exists and does not parse is reported;
    /// no project at all stays silent.
    fn config(&self, cx: &Cx) -> Config {
        let verb = if self.uninstall {
            "removing modules for"
        } else {
            "mirroring modules for"
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
        let mirror = Mirror::new(&config);
        if self.uninstall {
            mirror.uninstall()?.render(cx.ui);
            return Ok(());
        }
        let settings = mirror.install()?.render(cx.ui);
        settings.render(cx.ui);
        Ok(())
    }
}
