//! Conventional default values for [`crate::config::Config`].
//!
//! Defaults are chosen so a fresh `baudelaire init` site builds with zero
//! config. Every field has a sane value; `config.kdl` only overrides.

use std::path::PathBuf;

use crate::config::{
    AnnounceConfig, AssetConfig, CacheConfig, CollectionConfig, Config, DeployConfig, DraftConfig,
    FeedConfig, HooksConfig, HtmlConfig, ImagesConfig, JpegConfig, LinkConfig, LlmsConfig,
    OptimizeConfig, PngConfig, PngStrip, ResponsiveConfig, RobotsConfig, S3Config, SearchConfig,
    SearchField, ServeConfig, SshConfig, StandardConfig, VerifyConfig,
};

impl Default for Config {
    fn default() -> Self {
        Self {
            site: None,
            url: None,
            lang: "en".into(),
            author: None,
            // The process cwd, which `Root::enter` has already moved to the
            // project directory.
            root: PathBuf::from("."),
            content: PathBuf::from("content"),
            index: Some("index".into()),
            dist: PathBuf::from("public"),
            assets: PathBuf::from("assets"),
            r#static: PathBuf::from("static"),
            templates: PathBuf::from("templates"),
            urls: UrlStyle::default(),
            clean: true,
            future: false,
            // opt-in like robots/llms/search; also needs a `url`
            sitemap: false,
            robots: RobotsConfig::default(),
            llms: LlmsConfig::default(),
            draft: DraftConfig::default(),
            links: LinkConfig::default(),
            feed: FeedConfig::default(),
            search: SearchConfig::default(),
            inputs: Default::default(),
            client: Default::default(),
            // HTML is forced on in `world.rs`; this list is purely additive
            // extras, empty by default.
            features: Vec::new(),
            collections: Default::default(),
            taxonomies: Default::default(),
            languages: Default::default(),
            html: HtmlConfig::default(),
            images: ImagesConfig::default(),
            asset: AssetConfig::default(),
            cache: CacheConfig::default(),
            hooks: HooksConfig::default(),
            announce: AnnounceConfig::default(),
            deploy: DeployConfig::default(),
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
            // lazy loading is a universal win; optimization is opt-in: it re-encodes and costs time
            lazy: true,
            // externalize by default: smaller HTML and cacheable images, at full
            // sizing parity with typst's inlining. `html.embed` forces inline.
            extract: true,
            optimize: OptimizeConfig::default(),
            responsive: ResponsiveConfig::default(),
        }
    }
}

impl Default for ResponsiveConfig {
    fn default() -> Self {
        // opt-in (re-encodes, costs time); the default widths cover phone,
        // tablet, and desktop breakpoints when the block is present but silent.
        Self {
            enabled: false,
            widths: vec![480, 960, 1440],
            quality: 80,
            // no default: the browser already assumes 100vw for w-descriptor
            // srcsets, so emitting it would add bytes for nothing. A theme sets
            // its real content width here.
            sizes: None,
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
            // opt-in like search: no feed until a format is named
            formats: Vec::new(),
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

impl Default for S3Config {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            // None targets AWS; R2/custom hosts set it.
            endpoint: None,
            // AWS's default region; R2 wants "auto".
            region: "us-east-1".into(),
            prefix: String::new(),
            // reconcile: remove what the build no longer produces.
            delete: true,
        }
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            path: String::new(),
            // The standard SSH port.
            port: 22,
            // None resolves to $USER at deploy time.
            user: None,
            // None falls back to agent, then password auth.
            key: None,
            // secure by default: verify host keys against known_hosts.
            strict: true,
            // reconcile: remove what the build no longer produces.
            delete: true,
        }
    }
}

impl Default for StandardConfig {
    fn default() -> Self {
        Self {
            // empty by convention: a backend checks handle presence to know it was configured
            handle: String::new(),
            // resolved from the session at announce time; set in config only to unlock offline verify artifacts
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
    /// Directory-per-page: `foo.typ` -> `foo/index.html`, served at `/foo/`.
    #[default]
    Clean,
    /// Flat files: `foo.typ` -> `foo.html`, served at `/foo.html`.
    Flat,
}

impl UrlStyle {
    /// Shape a page URL for this style.
    ///
    /// This is the half that used to be missing: the style only decided the
    /// output *file*, while every permalink kept the clean trailing-slash form.
    /// A flat site wrote `about.html` and then told the canonical tag, `og:url`,
    /// the sitemap, the feeds, the redirects and every rewritten `.typ` link
    /// that the page lived at `/about/`, which nothing serves. The site root is
    /// `/` under both styles.
    pub fn url(self, path: &str) -> String {
        match self {
            Self::Clean => path.to_owned(),
            Self::Flat if path == "/" || path.ends_with(Self::PAGE) => path.to_owned(),
            Self::Flat => format!("{}{}", path.trim_end_matches('/'), Self::PAGE),
        }
    }

    /// The extension a flat URL names its file with.
    const PAGE: &'static str = ".html";
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
