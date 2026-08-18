//! `baudelaire config explain`: what one key is, and where this config sets it.

use std::path::Path;

use clap::Args;

use super::super::{Cx, Run, help};
use super::key;
use crate::error::Result;
use crate::ui::markup;

#[derive(Args, Debug, Clone)]
#[command(after_help = ExplainArgs::examples())]
pub struct ExplainArgs {
    /// The dotted key path, e.g. `content.collections.permalink`.
    #[arg(value_parser = key::Parser, hide_possible_values = true)]
    pub key: String,
}

impl ExplainArgs {
    /// Appended to `config explain --help`.
    fn examples() -> String {
        help::Table::examples(&[
            ("baudelaire config explain paths.dist", "One key"),
            (
                "baudelaire config explain content.collections.permalink",
                "A key under a name you chose",
            ),
        ])
        .to_string()
    }
}

impl Run for ExplainArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        use crate::config::explain::Sightings;
        use crate::config::reference::Reference;
        use crate::error::cli::UnknownKey;

        let reference = Reference::at(&self.key).ok_or_else(|| UnknownKey::at(&self.key))?;
        let entry = &reference.entries()[0];
        cx.ui
            .banner(markup!("`{}`  {}", &entry.path, entry.kind.label()));
        // The tables' own prose, which carries the code spans it was written with.
        cx.ui.detail(entry.doc);

        let config = cx.cli.config()?;
        let sightings = Sightings::of(&config, &self.key)?;
        if sightings.all().is_empty() {
            cx.ui
                .item("not set here: the built-in default is what applies");
            return Ok(());
        }
        for sighting in sightings.all() {
            cx.ui.item(Set(sighting, &cx.cli.global.config));
        }
        Ok(())
    }
}

/// One sighting as one line: where it is written, under whose name, and what it
/// says.
struct Set<'a>(&'a crate::config::explain::Sighting, &'a Path);

impl std::fmt::Display for Set<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self(sighting, path) = self;
        match &sighting.profile {
            Some(profile) => write!(f, "profile {profile}")?,
            None => write!(f, "{}:{}", path.display(), sighting.line)?,
        }
        if !sighting.named.is_empty() {
            write!(f, " [{}]", sighting.named.join(" "))?;
        }
        write!(f, "  {}", sighting.written)
    }
}
