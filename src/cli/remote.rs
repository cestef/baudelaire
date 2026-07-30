//! The terminal-facing half of the two publishing commands.
//!
//! `announce` and `deploy` differ only in which module they hand the built site
//! to; the build, the flag-to-[`Options`] mapping and the [`Tty`] interaction
//! are identical, so they live here once.

use crate::cli::prompt::Tty;
use crate::config::Config;
use crate::engine::{Engine, Mode};
use crate::error::Result;
use crate::remote::Options;
use crate::ui::Ui;

/// The flags every publishing command shares, so one driver serves both.
pub(crate) trait Flags {
    fn dry_run(&self) -> bool;
    fn yes(&self) -> bool;
    /// The secret supplied on the command line.
    fn secret(&self) -> Option<String>;
}

/// Build the site, then hand it to `publish`. A fresh build first, so a publish
/// always reflects the current sources.
pub(crate) fn run(
    ui: &Ui,
    config: &Config,
    args: &impl Flags,
    publish: fn(&Config, &Options, &Ui) -> Result<()>,
) -> Result<()> {
    Engine::new(config.clone(), Mode::Build)?.build(ui)?;
    let tty = Tty;
    let options = Options {
        dry_run: args.dry_run(),
        yes: args.yes(),
        secret: args.secret(),
        interaction: &tty,
    };
    publish(config, &options, ui)
}
