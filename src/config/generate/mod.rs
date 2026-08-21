//! `generate { }`: the files a build emits beside the pages.

pub mod feed;
pub mod llms;
pub mod manifest;
pub mod robots;
pub mod search;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;
use crate::config::{FeedConfig, LlmsConfig, ManifestConfig, RobotsConfig, SearchConfig};

/// Each field is opt-in: either a flag or a block whose presence turns it on.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct GenerateConfig {
    /// Write `sitemap.xml`.
    ///
    /// Needs `url` set.
    #[key(flag)]
    pub sitemap: bool,

    /// Write `robots.txt`. Its presence turns it on; `#false` turns it off again.
    #[key(nested(RobotsConfig))]
    pub robots: RobotsConfig,

    /// Write `llms.txt`. Its presence turns it on; `#false` turns it off again.
    #[key(nested(LlmsConfig))]
    pub llms: LlmsConfig,

    /// Write `manifest.webmanifest`. Its presence turns it on; `#false` turns it off again.
    #[key(nested(ManifestConfig))]
    pub manifest: ManifestConfig,

    /// Write syndication feeds.
    #[key(nested(FeedConfig))]
    pub feed: FeedConfig,

    /// Write a client-side search index.
    #[key(nested(SearchConfig))]
    pub search: SearchConfig,
}
