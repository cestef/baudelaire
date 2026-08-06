//! `prune { }`: the sweep that deletes what a build did not produce, and what
//! it is told to leave behind.

use crate::config::dispatch::Kind::Texts;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// The sweep over the output directory: whether it runs, and the paths it may
/// not touch.
///
/// `keep` exists for a `dist` that receives output from something other than
/// this build: a second site published under a subdirectory, a directory of
/// artifacts another tool writes there. Without it the only way to combine the
/// two is to order them so the pruning build runs first, which is an ordering
/// nothing enforces and a mistake nothing reports.
#[derive(Debug, Clone, Hash)]
pub struct PruneConfig {
    /// Whether the sweep runs at all.
    pub enabled: bool,
    /// Globs, relative to the output directory, that the sweep keeps whether or
    /// not this build produced them.
    pub keep: Vec<String>,
}

/// On, and keeping nothing beyond what the build itself wrote. The sweep is the
/// default because a `dist` that only grows serves deleted pages.
impl Default for PruneConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            keep: Vec::new(),
        }
    }
}

impl Section for PruneConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[(
        "keep",
        Texts,
        "Globs, relative to the output directory, that the sweep never deletes: `keep \"themes/**\"`.",
        |c, n, t| {
            c.keep = n.words(t)?;
            Ok(())
        },
    )]);
}
