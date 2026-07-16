//! `baudelaire deploy`: build the site, then upload its files to every configured
//! destination. The terminal-facing half of [`crate::deploy`] — it maps the CLI
//! flags onto [`Options`] and supplies an [`Interaction`] backed by the shared
//! prompt widgets, so confirmation and credential entry reuse the same styled
//! prompts as the rest of the CLI.

use crate::cli::prompt::{Prompt, Secret};
use crate::config::Config;
use crate::engine::{Engine, Mode};
use crate::error::Result;
use crate::remote::{Interaction, Options};
use crate::ui::Ui;

use super::DeployArgs;

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

/// Build the site, then deploy its files. A fresh build first, so a deploy always
/// reflects the current sources.
pub fn run(ui: &Ui, config: &Config, args: &DeployArgs) -> Result<()> {
    Engine::new(config.clone(), Mode::Build)?.build(ui)?;
    let tty = Tty;
    let options = Options {
        dry_run: args.dry_run,
        yes: args.yes,
        secret: args.secret.clone(),
        interaction: &tty,
    };
    crate::deploy::run(config, &options, ui)
}
