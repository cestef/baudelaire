//! Conventional default values for [`crate::config::Config`].
//!
//! Defaults are chosen so a fresh `baudelaire init` site builds with zero
//! config. Every field has a sane value; `config.kdl` only overrides.

use std::path::PathBuf;

use crate::config::{
    AssetConfig, CacheConfig, CollectionConfig, Config, DraftConfig, FeedConfig, FeedKind,
    HooksConfig, HtmlConfig, ImagesConfig, JpegConfig, LinkConfig, LlmsConfig, OptimizeConfig,
    PngConfig, PngStrip, PublishConfig, RobotsConfig, SearchConfig, SearchField, ServeConfig,
    StandardConfig, VerifyConfig,
};

impl Default for Config {
    fn default() -> Self {
        Self {
            site: None,
            url: None,
            lang: "en".into(),
            author: None,
            content: PathBuf::from("content"),
            index: Some("index".into()),
            dist: PathBuf::from("public"),
            assets: PathBuf::from("assets"),
            r#static: PathBuf::from("static"),
            templates: PathBuf::from("templates"),
            urls: UrlStyle::default(),
            clean: true,
            future: false,
            sitemap: true,
            robots: RobotsConfig::default(),
            llms: LlmsConfig::default(),
            draft: DraftConfig::default(),
            links: LinkConfig::default(),
            feed: FeedConfig::default(),
            search: SearchConfig::default(),
            inputs: Default::default(),
            client: Default::default(),
            features: vec!["html".into()],
            collections: Default::default(),
            taxonomies: Default::default(),
            html: HtmlConfig::default(),
            images: ImagesConfig::default(),
            asset: AssetConfig::default(),
            cache: CacheConfig::default(),
            hooks: HooksConfig::default(),
            publish: PublishConfig::default(),
            serve: ServeConfig::default(),
            profile: None,
            profiles: Default::default(),
            source: String::new(),
        }
    }
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self {
            pretty: true,
            embed: false,
            meta: true,
            anchors: true,
        }
    }
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            // lazy loading is a universal win; optimization is opt-in — it re-encodes and costs time
            lazy: true,
            optimize: OptimizeConfig::default(),
        }
    }
}

impl Default for PngConfig {
    fn default() -> Self {
        // preset 2 balances savings and speed; Safe strips only non-rendering metadata
        Self {
            level: 2,
            strip: PngStrip::Safe,
        }
    }
}

impl Default for JpegConfig {
    fn default() -> Self {
        // 82 is a widely used "visually lossless" default
        Self { quality: 82 }
    }
}

impl Default for DraftConfig {
    fn default() -> Self {
        Self {
            build: false,
            suffix: ".draft".into(),
        }
    }
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self { strict: true }
    }
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self {
            formats: vec![FeedKind::Rss],
            limit: 20,
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            // opt-in: no index until a format is configured
            formats: Vec::new(),
            fields: vec![SearchField::Title, SearchField::Body, SearchField::Tags],
            stopwords: Vec::new(),
            min_length: 2,
            client: false,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            dir: Config::scratch("cache"),
            incremental: true,
        }
    }
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            port: 1821,
            bind: "127.0.0.1".into(),
            open: true,
            watch: true,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

impl Default for StandardConfig {
    fn default() -> Self {
        Self {
            // empty by convention: a backend checks handle presence to know it was configured
            handle: String::new(),
            // resolved from the session at publish; set in config only to unlock offline verify artifacts
            did: None,
            // Bluesky entryway, also the PDS for accounts it hosts; custom-PDS users override
            pds: "https://bsky.social".into(),
            discover: true,
            icon: None,
            verify: VerifyConfig::default(),
        }
    }
}

impl Default for VerifyConfig {
    fn default() -> Self {
        // both on: with a `did` set, a site should verify unless it opts out
        Self {
            wellknown: true,
            links: true,
        }
    }
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            glob: None,
            sort: SortKey::Order,
            reverse: false,
            permalink: None,
            template: None,
            paginate: None,
            list: None,
            index: None,
            prefix: "page".into(),
        }
    }
}

/// How page permalinks map onto output files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UrlStyle {
    /// Directory-per-page: `foo.typ` → `foo/index.html`, served at `/foo/`.
    #[default]
    Clean,
    /// Flat files: `foo.typ` → `foo.html`, served at `/foo.html`.
    Flat,
}

impl UrlStyle {
    /// Whether this style produces directory-per-page (clean) URLs.
    pub fn is_clean(self) -> bool {
        matches!(self, UrlStyle::Clean)
    }
}

/// Ordering key for a collection's pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SortKey {
    /// Frontmatter `order` field, ascending.
    #[default]
    Order,
    /// Frontmatter `date` field, ascending.
    Date,
    /// Frontmatter `title` field, alphabetical.
    Title,
}
