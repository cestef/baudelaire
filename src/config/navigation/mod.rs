//! `navigation { }`: how a visitor moves between the built pages.

pub mod spa;
pub mod speculation;
pub mod standalone;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::{Block, Section};
use crate::config::{SpaConfig, SpeculationConfig, StandaloneConfig};

/// How a visitor moves between the built pages. Three independent strategies,
/// each enabled by the presence of its block.
#[derive(Debug, Clone, Hash, Default)]
pub struct NavigationConfig {
    pub spa: SpaConfig,
    pub standalone: StandaloneConfig,
    pub speculation: SpeculationConfig,
}

impl Section for NavigationConfig {
    const RULES: Block<Self> = Block(&[
        (
            "spa",
            Nested(SpaConfig::rows),
            "Client-side navigation between pages. Its presence turns it on; `#false` turns it off again.",
            |c| c.spa.values(),
            |c, n, t| c.spa.fill(n, t),
        ),
        (
            "standalone",
            Nested(StandaloneConfig::rows),
            "Export the whole site as one HTML file. Its presence turns it on; `#false` turns it off again.",
            |c| c.standalone.values(),
            |c, n, t| c.standalone.fill(n, t),
        ),
        (
            "speculation",
            Nested(SpeculationConfig::rows),
            "Browser prefetch and prerender hints. Its presence turns them on; `#false` turns them off again.",
            |c| c.speculation.values(),
            |c, n, t| c.speculation.fill(n, t),
        ),
    ]);
}
