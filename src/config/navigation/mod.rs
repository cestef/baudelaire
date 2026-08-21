//! `navigation { }`: how a visitor moves between the built pages.

pub mod spa;
pub mod speculation;
pub mod standalone;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;
use crate::config::{SpaConfig, SpeculationConfig, StandaloneConfig};

/// How a visitor moves between the built pages. Three independent strategies,
/// each enabled by the presence of its block.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct NavigationConfig {
    /// Client-side navigation between pages. Its presence turns it on; `#false` turns it off again.
    #[key(nested(SpaConfig))]
    pub spa: SpaConfig,

    /// Export the whole site as one HTML file. Its presence turns it on; `#false` turns it off again.
    #[key(nested(StandaloneConfig))]
    pub standalone: StandaloneConfig,

    /// Browser prefetch and prerender hints. Its presence turns them on; `#false` turns them off again.
    #[key(nested(SpeculationConfig))]
    pub speculation: SpeculationConfig,
}
