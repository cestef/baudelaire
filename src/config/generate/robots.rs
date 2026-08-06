//! `generate { robots { } }`: `robots.txt`.

use crate::config::dispatch::Kind::Texts;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// `robots.txt` generation. Enabled by the presence of a `generate { robots }`
/// block.
#[derive(Debug, Clone, Hash, Default)]
pub struct RobotsConfig {
    /// Whether to emit `robots.txt`.
    pub enabled: bool,
    /// Paths disallowed for all crawlers. Empty = allow everything.
    pub disallow: Vec<String>,
}

impl Section for RobotsConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[(
        "disallow",
        Texts,
        "Paths to disallow, one word each.",
        |c, n, t| {
            c.disallow = n.words(t)?;
            Ok(())
        },
    )]);
}
