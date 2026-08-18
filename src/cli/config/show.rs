//! `baudelaire config show`: the config, or one block of it, highlighted.

use clap::Args;

use super::super::{Cx, Run};
use super::key;
use crate::config::values::{Tree, Value};
use crate::error::Result;
use crate::error::cli::{Generated, UnsetKey};
use crate::ui::Highlighted;

#[derive(Args, Debug, Clone)]
pub struct ShowArgs {
    /// A dotted key path to narrow to; the whole config without one.
    #[arg(value_parser = key::Parser, hide_possible_values = true)]
    pub key: Option<String>,

    /// Print what the config resolves to, defaults included, rather than the
    /// text as written.
    #[arg(long)]
    pub effective: bool,
}

impl Run for ShowArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let kdl = if self.effective {
            self.resolved(cx)?
        } else {
            self.authored(cx)?
        };
        let highlighted = Highlighted::new(kdl.trim_end(), "kdl");
        Generated::Value.emit(format!("{highlighted}\n").as_bytes())?;
        Ok(())
    }
}

impl ShowArgs {
    /// Every layer resolved, written back as the config that would parse to it.
    fn resolved(&self, cx: &Cx) -> Result<String> {
        let sources = cx.cli.sources()?;
        let Some(key) = &self.key else {
            return Ok(Tree::of(sources.values()).to_string());
        };
        let resolved = sources
            .resolve(key)
            .filter(|resolved| !resolved.value.is_empty())
            .ok_or_else(|| UnsetKey { key: key.clone() })?;
        let named = key.rsplit('.').next().unwrap_or(key).to_owned();
        let under = Value::block(vec![(named, resolved.value.clone())]);
        Ok(Tree::of(&under).to_string())
    }

    /// The config text itself, or every place it writes one key.
    fn authored(&self, cx: &Cx) -> Result<String> {
        use crate::config::explain::Sightings;

        let config = cx.cli.config()?;
        let Some(key) = &self.key else {
            return Ok(config.text().to_owned());
        };
        let sightings = Sightings::of(&config, key)?;
        if sightings.all().is_empty() {
            return Err(UnsetKey { key: key.clone() }.into());
        }
        let blocks: Vec<&str> = sightings
            .all()
            .iter()
            .map(|sighting| sighting.block.as_str())
            .collect();
        Ok(format!("{}\n", blocks.join("\n")))
    }
}
