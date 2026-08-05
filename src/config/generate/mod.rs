//! `generate { }`: the files a build emits beside the pages.

pub mod cards;
pub mod feed;
pub mod llms;
pub mod manifest;
pub mod pdf;
pub mod robots;
pub mod search;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{
    CardsConfig, FeedConfig, LlmsConfig, ManifestConfig, PdfConfig, RobotsConfig, SearchConfig,
};

/// The files a build emits beside the pages themselves. Each one is opt-in:
/// either a flag or a block whose presence turns it on.
#[derive(Debug, Clone, Hash, Default)]
pub struct GenerateConfig {
    /// Emit `sitemap.xml`. Opt-in like its neighbours, and needs a `url`.
    pub sitemap: bool,
    /// Emit a `_headers` rule file stating the `caching` policy to the host.
    pub headers: bool,
    /// Emit a `_redirects` rule file instead of the per-path HTML stubs.
    ///
    /// Netlify and Cloudflare Pages read it from the publish directory and
    /// answer with a real 301, where a stub is a client-side round trip that
    /// passes link equity worse. It *replaces* the stubs rather than joining
    /// them: both hosts serve a static file in preference to a redirect rule,
    /// so a stub sitting at the old path would win and the 301 would never
    /// fire.
    pub redirects: bool,
    /// `robots.txt` generation.
    pub robots: RobotsConfig,
    /// `llms.txt` generation.
    pub llms: LlmsConfig,
    /// `manifest.webmanifest` generation.
    pub manifest: ManifestConfig,
    /// Syndication feeds.
    pub feed: FeedConfig,
    /// Client-side search indexes.
    pub search: SearchConfig,
    /// Generated social cards.
    pub cards: CardsConfig,
    /// A PDF of every page, beside its HTML.
    pub pdf: PdfConfig,
}

/// The `generate { .. }` section: the files a build emits beside the pages.
/// Each child is opt-in, either a flag or a block whose presence turns it on.
impl Section for GenerateConfig {
    const RULES: Block<Self> = Block(&[
        ("sitemap", Flag, "Write `sitemap.xml`.", |c, n, t| {
            c.sitemap = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "redirects",
            Flag,
            "Write a `_redirects` file from each page's declared aliases.",
            |c, n, t| {
                c.redirects = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "headers",
            Flag,
            "Write a `_headers` file from the caching policy.",
            |c, n, t| {
                c.headers = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "robots",
            Nested(RobotsConfig::rows),
            "Write `robots.txt`. Its presence turns it on.",
            |c, n, t| c.robots.fill(n, t),
        ),
        (
            "llms",
            Nested(LlmsConfig::rows),
            "Write `llms.txt`. Its presence turns it on.",
            |c, n, t| c.llms.fill(n, t),
        ),
        (
            "manifest",
            Nested(ManifestConfig::rows),
            "Write `manifest.webmanifest`. Its presence turns it on.",
            |c, n, t| c.manifest.fill(n, t),
        ),
        (
            "feed",
            Nested(FeedConfig::rows),
            "Write syndication feeds.",
            |c, n, t| c.feed.fill(n, t),
        ),
        (
            "search",
            Nested(SearchConfig::rows),
            "Write a client-side search index.",
            |c, n, t| c.search.fill(n, t),
        ),
        (
            "cards",
            Nested(CardsConfig::rows),
            "Draw a social card per page. Its presence turns it on.",
            |c, n, t| c.cards.fill(n, t),
        ),
        (
            "pdf",
            Nested(PdfConfig::rows),
            "Typeset PDFs beside the pages.",
            |c, n, t| c.pdf.fill(n, t),
        ),
    ]);
}
