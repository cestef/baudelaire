//! `baudelaire config get`: what this config sets one key to, for a script.

use clap::Args;

use super::super::{Cx, Run};
use super::key;
use crate::error::Result;
use crate::error::cli::{Generated, UnsetKey};

#[derive(Args, Debug, Clone)]
pub struct GetArgs {
    /// The dotted key path, e.g. `paths.dist`.
    #[arg(value_parser = key::Parser, hide_possible_values = true)]
    pub key: String,
}

impl Run for GetArgs {
    /// One value per line on stdout, nothing at all for a key the config never
    /// set: what a script reads, so no decoration and no banner.
    fn run(&self, cx: &Cx) -> Result<()> {
        use crate::config::explain::Sightings;

        let config = cx.cli.config()?;
        let sightings = Sightings::of(&config, &self.key)?;
        if sightings.all().is_empty() {
            return Err(UnsetKey {
                key: self.key.clone(),
            }
            .into());
        }
        let written: Vec<&str> = sightings
            .all()
            .iter()
            .map(|sighting| sighting.scalar.as_str())
            .collect();
        Generated::Value.emit(format!("{}\n", written.join("\n")).as_bytes())?;
        Ok(())
    }
}
