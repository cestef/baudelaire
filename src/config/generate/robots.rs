//! `generate { robots { } }`: `robots.txt`.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// `robots.txt` generation. Enabled by the presence of a `generate { robots }`
/// block.
#[derive(Debug, Clone, Hash, Default, Table)]
#[table(hook(switch = enabled))]
pub struct RobotsConfig {
    pub enabled: bool,

    /// Paths to disallow, one word each.
    ///
    /// Empty allows everything.
    #[key(texts)]
    pub disallow: Vec<String>,
}
