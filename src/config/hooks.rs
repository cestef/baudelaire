//! `hooks { }`: external commands run around the build.

use crate::config::dispatch::Kind::Texts;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// External command hooks, each run through the system shell in the project
/// root.
#[derive(Debug, Clone, Hash, Default)]
pub struct HooksConfig {
    /// Run before the asset pipeline, so files they generate into `assets/` are
    /// picked up and fingerprinted.
    pub before: Vec<String>,
    pub after: Vec<String>,
}

impl Section for HooksConfig {
    const RULES: Block<Self> = Block(&[
        (
            "before",
            Texts,
            "Commands run before the asset pipeline, so what they generate is picked up.",
            |c| c.before.clone().into(),
            |c, n, t| {
                c.before = n.words(t)?;
                Ok(())
            },
        ),
        (
            "after",
            Texts,
            "Commands run once the output directory is written.",
            |c| c.after.clone().into(),
            |c, n, t| {
                c.after = n.words(t)?;
                Ok(())
            },
        ),
    ]);
}
