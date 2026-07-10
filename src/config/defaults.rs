//! Conventional default values for [`crate::config::Config`].
//!
//! Defaults are chosen so a fresh `baudelaire init` site builds with zero
//! config. Every field has a sane value; `config.kdl` only overrides.

use std::path::PathBuf;

use crate::config::{
    AssetConfig, CacheConfig, CollectionConfig, Config, DraftConfig, FeedConfig, FeedKind,
    HooksConfig, HtmlConfig, ImagesConfig, JpegConfig, LinkConfig, LlmsConfig, OptimizeConfig,
    PngConfig, PngStrip, RobotsConfig, SearchConfig, SearchField, ServeConfig,
};

impl Default for Config {
    fn default() -> Self {
        Self {
            site: None,
            url: None,
            lang: "en".into(),
            author: None,
            content: PathBuf::from("content"),
            dist: PathBuf::from("public"),
            assets: PathBuf::from("assets"),
            templates: PathBuf::from("templates"),
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
            features: vec!["html".into()],
            collections: Default::default(),
            taxonomies: Default::default(),
            html: HtmlConfig::default(),
            images: ImagesConfig::default(),
            asset: AssetConfig::default(),
            cache: CacheConfig::default(),
            hooks: HooksConfig::default(),
            serve: ServeConfig::default(),
            profile: None,
            profiles: Default::default(),
        }
    }
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self {
            pretty: true,
            embed: false,
            meta: true,
        }
    }
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            // Lazy loading is a safe, universal win; optimization is opt-in (an
            // empty `optimize` block) since it re-encodes files and costs time.
            lazy: true,
            optimize: OptimizeConfig::default(),
        }
    }
}

impl Default for PngConfig {
    fn default() -> Self {
        // Preset 2 is a good balance of savings and speed; Safe strips metadata
        // without touching anything that affects rendering.
        Self { level: 2, strip: PngStrip::Safe }
    }
}

impl Default for JpegConfig {
    fn default() -> Self {
        // 82 is a widely used "visually lossless" default.
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
            // Opt-in: no index emitted until a format is configured.
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
            dir: PathBuf::from(Config::SCRATCH).join("cache"),
            incremental: true,
        }
    }
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            bind: "127.0.0.1".into(),
            open: true,
            watch: true,
            include: Vec::new(),
            exclude: Vec::new(),
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
        }
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
