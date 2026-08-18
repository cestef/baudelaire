//! `prune { }`: the sweep that deletes what a build did not produce.

use crate::config::dispatch::Kind::Texts;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// The sweep over the output directory: whether it runs, and the paths it may
/// not touch.
#[derive(Debug, Clone, Hash)]
pub struct PruneConfig {
    pub enabled: bool,
    /// Globs, relative to the output directory, that the sweep keeps whether or
    /// not this build produced them.
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

impl Section for PruneConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[(
        "keep",
        Texts,
        "Globs, relative to the output directory, that the sweep never deletes: `keep \"themes/**\"`.",
        |c| c.keep.clone().into(),
        |c, n, t| {
            c.keep = n.words(t)?;
            Ok(())
        },
    )]);
}
