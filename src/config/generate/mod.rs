//! `generate { }`: the files a build emits beside the pages.

pub mod bundle;
pub mod cards;
pub mod feed;
pub mod llms;
pub mod manifest;
pub mod pdf;
pub mod robots;
pub mod search;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::{Flag, Items};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{
    BundleConfig, CardsConfig, FeedConfig, LlmsConfig, ManifestConfig, PdfConfig, RobotsConfig,
    SearchConfig, Value,
};

/// Each field is opt-in: either a flag or a block whose presence turns it on.
#[derive(Debug, Clone, Hash, Default)]
pub struct GenerateConfig {
    /// Emit `sitemap.xml`, which needs `url` set.
    pub sitemap: bool,
    /// Emit a `_redirects` file in place of the per-path HTML stubs; both
    /// Netlify and Cloudflare Pages serve a static file over a redirect rule,
    /// so a stub would shadow the rule if the two coexisted.
    pub redirects: bool,
    pub robots: RobotsConfig,
    pub llms: LlmsConfig,
    pub manifest: ManifestConfig,
    pub feed: FeedConfig,
    pub search: SearchConfig,
    pub cards: CardsConfig,
    pub pdf: PdfConfig,
    /// Documents bound from many pages, keyed by the filename stem every format
    /// of that bundle is written under.
    pub bundles: Vec<(String, BundleConfig)>,
}

impl Section for GenerateConfig {
    const RULES: Block<Self> = Block(&[
        (
            "sitemap",
            Flag,
            "Write `sitemap.xml`.",
            |c| c.sitemap.into(),
            |c, n, t| {
                c.sitemap = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "redirects",
            Flag,
            "Write a `_redirects` file from each page's declared aliases.",
            |c| c.redirects.into(),
            |c, n, t| {
                c.redirects = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "robots",
            Nested(RobotsConfig::rows),
            "Write `robots.txt`. Its presence turns it on; `#false` turns it off again.",
            |c| c.robots.values(),
            |c, n, t| c.robots.fill(n, t),
        ),
        (
            "llms",
            Nested(LlmsConfig::rows),
            "Write `llms.txt`. Its presence turns it on; `#false` turns it off again.",
            |c| c.llms.values(),
            |c, n, t| c.llms.fill(n, t),
        ),
        (
            "manifest",
            Nested(ManifestConfig::rows),
            "Write `manifest.webmanifest`. Its presence turns it on; `#false` turns it off again.",
            |c| c.manifest.values(),
            |c, n, t| c.manifest.fill(n, t),
        ),
        (
            "feed",
            Nested(FeedConfig::rows),
            "Write syndication feeds.",
            |c| c.feed.values(),
            |c, n, t| c.feed.fill(n, t),
        ),
        (
            "search",
            Nested(SearchConfig::rows),
            "Write a client-side search index.",
            |c| c.search.values(),
            |c, n, t| c.search.fill(n, t),
        ),
        (
            "cards",
            Nested(CardsConfig::rows),
            "Draw a social card per page. Its presence turns it on; `#false` turns it off again.",
            |c| c.cards.values(),
            |c, n, t| c.cards.fill(n, t),
        ),
        (
            "pdf",
            Nested(PdfConfig::rows),
            "Typeset PDFs beside the pages.",
            |c| c.pdf.values(),
            |c, n, t| c.pdf.fill(n, t),
        ),
        (
            "bundles",
            Items(BundleConfig::rows),
            "One block per bound document, each named by the id its files are written under.",
            |c| Value::each(&c.bundles, Section::values),
            |c, n, t| {
                c.bundles = n.unique(t, "bundle", BundleConfig::item)?;
                Ok(())
            },
        ),
    ]);
}
