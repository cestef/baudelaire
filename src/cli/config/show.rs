//! `baudelaire config show`: the config, or one block of it, highlighted.

use clap::Args;

use super::super::{Cx, Run};
use super::key;
use crate::error::Result;
use crate::error::cli::{Generated, UnsetKey};
use crate::ui::Highlighted;

#[derive(Args, Debug, Clone)]
pub struct ShowArgs {
    /// A dotted key path to narrow to; the whole config without one.
    #[arg(value_parser = key::Parser, hide_possible_values = true)]
    pub key: Option<String>,
}

impl Run for ShowArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        use crate::config::explain::Sightings;

        let config = cx.cli.config()?;
        let Some(key) = &self.key else {
            Generated::Value.emit(
                Highlighted::new(config.text(), "kdl")
                    .to_string()
                    .as_bytes(),
            )?;
            return Ok(());
        };
        let sightings = Sightings::of(&config, key)?;
        if sightings.all().is_empty() {
            return Err(UnsetKey { key: key.clone() }.into());
        }
        for sighting in sightings.all() {
            let block = Highlighted::new(&sighting.block, "kdl").to_string();
            Generated::Value.emit(format!("{block}\n").as_bytes())?;
        }
        Ok(())
    }
}
