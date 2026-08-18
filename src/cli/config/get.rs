//! `baudelaire config get`: what one key is set to, for a script.

use clap::Args;

use super::super::{Cx, Run};
use super::key;
use crate::error::Result;
use crate::error::cli::{Generated, UnsetKey};
use crate::ui::Highlighted;

#[derive(Args, Debug, Clone)]
pub struct GetArgs {
    /// The dotted key path, e.g. `paths.dist`.
    #[arg(value_parser = key::Parser, hide_possible_values = true)]
    pub key: String,

    /// Print only what the config file itself writes, not the effective value.
    #[arg(long)]
    pub written: bool,
}

impl Run for GetArgs {
    /// The effective value, one per line, and nothing at all for a key that
    /// holds nothing: what a script reads, so no decoration and no banner.
    fn run(&self, cx: &Cx) -> Result<()> {
        let written = if self.written {
            self.authored(cx)?
        } else {
            self.effective(cx)?
        };
        let highlighted = Highlighted::new(&written, "kdl");
        Generated::Value.emit(format!("{highlighted}\n").as_bytes())?;
        Ok(())
    }
}

impl GetArgs {
    /// The value every layer resolves to, defaults included.
    fn effective(&self, cx: &Cx) -> Result<String> {
        use crate::config::Value;

        let sources = cx.cli.sources()?;
        let resolved = sources.resolve(&self.key).ok_or_else(|| self.unset())?;
        let written = resolved.value.scalar();
        if written.is_empty() && !matches!(resolved.value, Value::List(_)) {
            return Err(self.unset().into());
        }
        Ok(written)
    }

    /// Every place the config text itself writes the key, in the order written.
    fn authored(&self, cx: &Cx) -> Result<String> {
        use crate::config::explain::Sightings;

        let config = cx.cli.config()?;
        let sightings = Sightings::of(&config, &self.key)?;
        if sightings.all().is_empty() {
            return Err(self.unset().into());
        }
        let written: Vec<&str> = sightings
            .all()
            .iter()
            .map(|sighting| sighting.scalar.as_str())
            .collect();
        Ok(written.join("\n"))
    }

    fn unset(&self) -> UnsetKey {
        UnsetKey {
            key: self.key.clone(),
        }
    }
}
