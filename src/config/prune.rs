//! `prune { }`: the sweep that deletes what a build did not produce.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// The sweep over the output directory: whether it runs, and the paths it may
/// not touch.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct PruneConfig {
    pub enabled: bool,

    /// Globs, relative to the output directory, that the sweep never deletes: `keep "themes/**"`.
    #[key(texts)]
    pub keep: Vec<String>,
}

impl Default for PruneConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            keep: Vec::new(),
        }
    }
}
