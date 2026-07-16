//! `baudelaire announce`: build the site, then push it to every configured
//! destination. The terminal-facing half of [`crate::announce`] — it maps the
//! CLI flags onto [`Options`] and supplies an [`Interaction`] backed by the
//! shared prompt widgets, so confirmation and secret entry reuse the same
//! styled prompts as `init` rather than re-implementing them.

use crate::cli::prompt::{Prompt, Secret};
use crate::config::Config;
use crate::engine::{Engine, Mode};
use crate::error::Result;
use crate::announce::{Interaction, Options};
use crate::ui::Ui;

use super::AnnounceArgs;

/// Terminal-backed [`Interaction`]: yes/no confirmation through [`Prompt`] and
/// hidden secret entry through [`Secret`].
struct Tty;

impl Interaction for Tty {
    fn confirm(&self, prompt: &str) -> Result<bool> {
        Prompt::new(prompt)
            .option(&["yes"], true)
            .option(&["no"], false)
            .default()
            .ask()
    }

    fn secret(&self, label: &str) -> Result<Option<String>> {
        Secret::new(label).ask()
    }
}

/// Build the site, then announce it. A fresh build first, so a announce always
/// reflects the current sources.
pub fn run(ui: &Ui, config: &Config, args: &AnnounceArgs) -> Result<()> {
    Engine::new(config.clone(), Mode::Build)?.build(ui)?;
    let tty = Tty;
    let options = Options {
        dry_run: args.dry_run,
        yes: args.yes,
        secret: args.password.clone(),
        interaction: &tty,
    };
    crate::announce::run(config, &options, ui)
}
