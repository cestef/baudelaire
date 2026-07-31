//! Conventional default values for [`crate::config::Config`].
//!
//! Defaults are chosen so a fresh `baudelaire init` site builds with zero
//! config. Every field has a sane value; `config.kdl` only overrides.

use std::path::PathBuf;

use crate::config::{
    AnnounceConfig, AssetConfig, CacheConfig, CacheControl, CardsConfig, CollectionConfig, Config,
    ContentConfig, DeployConfig, DraftConfig, Eagerness, FeedConfig, Footnotes, GenerateConfig,
    HighlightConfig, HooksConfig, HtmlConfig, ImagesConfig, JpegConfig, LinkConfig,
    NavigationConfig, OptimizeConfig, PaginateConfig, Paths, PdfBundle, PdfPages, PngConfig,
    PngStrip, Prefetch, ResponsiveConfig, Router, S3Config, SearchConfig, SearchField, ServeConfig,
    SortKey, SpaConfig, SpeculationConfig, SshConfig, StandaloneConfig, StandardConfig,
    TaxonomyConfig, TypstConfig, UrlStyle, VerifyConfig,
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
            paths: Paths::default(),
            theme: None,
            content: ContentConfig::default(),
            languages: Default::default(),
            assets: AssetConfig::default(),
            html: HtmlConfig::default(),
            links: LinkConfig::default(),
            generate: GenerateConfig::default(),
            navigation: NavigationConfig::default(),
            prune: true,
            typst: TypstConfig::default(),
            client: Default::default(),
            cache: CacheConfig::default(),
            caching: CacheControl::default(),
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

impl Default for Paths {
    fn default() -> Self {
        Self {
            content: PathBuf::from("content"),
            dist: PathBuf::from("public"),
            assets: PathBuf::from("assets"),
            r#static: PathBuf::from("static"),
            templates: PathBuf::from("templates"),
        }
    }
}

impl Default for ContentConfig {
    fn default() -> Self {
        Self {
            index: Some("index".into()),
            future: false,
            draft: DraftConfig::default(),
            collections: Default::default(),
            taxonomies: Default::default(),
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
            highlight: HighlightConfig::default(),
            // opt-in: structured data is a claim about the page, not a restating
            // of what it already says.
            jsonld: false,
            footnotes: Footnotes::default(),
            // opt-in: source spans are scaffolding for whoever is writing the
            // page, and every reader of the published one would pay for them.
            spans: false,
        }
    }
}

impl Default for StandaloneConfig {
    fn default() -> Self {
        // opt-in: the presence of a `standalone { .. }` block flips `enabled`.
        // The rest are the values that block inherits when it stays silent.
        Self {
            enabled: false,
            file: "site.html".into(),
            // no default entry: the site home, resolved against the pages that
            // actually built, so a site with no `/` still gets a shell.
            entry: None,
            router: Router::default(),
        }
    }
}

impl Default for SpaConfig {
    fn default() -> Self {
        // opt-in like `standalone`. `main` is the element typst-html templates
        // conventionally wrap page content in; anything shared (header, nav)
        // sits outside it and survives a navigation.
        Self {
            enabled: false,
            root: "main".into(),
            prefetch: Prefetch::default(),
        }
    }
}

impl Default for SpeculationConfig {
    fn default() -> Self {
        // opt-in like its neighbours. When the block is present but silent:
        // prefetch on hover (cheap, near-certain to be used) and no prerender,
        // which costs a full hidden page render and runs the target's scripts.
        Self {
            enabled: false,
            prefetch: Eagerness::Moderate,
            prerender: Eagerness::None,
        }
    }
}

impl Default for CardsConfig {
    fn default() -> Self {
        // opt-in: rendering a page per card is the most expensive thing a build
        // can do per page. 1200x630 is the size every unfurler crops to.
        Self {
            enabled: false,
            template: "card.typ".into(),
            width: 1200,
            height: 630,
        }
    }
}

impl Default for PdfPages {
    fn default() -> Self {
        // opt-in for the same reason a card is: it is a second compile of every
        // page, and this one lays the whole document out rather than one page.
        Self {
            enabled: false,
            template: "print.typ".into(),
        }
    }
}

impl Default for PdfBundle {
    fn default() -> Self {
        // No target: a bundle binds what the block names, and naming nothing
        // is asking for nothing. The template is the value a `bundle { }` block
        // inherits when it stays silent about it.
        Self {
            present: false,
            template: "book.typ".into(),
            collections: Vec::new(),
            site: false,
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
        // Strict internal links by default: a `.typ` link naming no page is a
        // typo, and the build knows it for certain. External checking is opt-in
        // and needs the network, so it can never be a default.
        Self {
            style: UrlStyle::default(),
            strict: true,
            external: false,
        }
    }
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self {
            // opt-in like search: no feed until a format is named
            formats: Vec::new(),
            limit: 20,
            // off by default: one more file per term per format multiplies the
            // output of a heavily tagged site
            terms: false,
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
            ui: false,
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
            // No guess: launching a program is the author's instruction, and
            // `$EDITOR` is as likely to be a terminal editor with no terminal
            // to appear in.
            editor: Vec::new(),
        }
    }
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            // None targets AWS; R2/custom hosts set it.
            endpoint: None,
            // Resolved from the target by `S3Config::region` when unset.
            region: None,
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
            paginate: PaginateConfig::default(),
        }
    }
}

impl Default for PaginateConfig {
    fn default() -> Self {
        Self {
            // opt-in: the presence of a `paginate { .. }` block flips it, and a
            // collection with no index generates none.
            enabled: false,
            // no size is one page holding every member
            size: None,
            template: None,
            mount: None,
            prefix: "page".into(),
        }
    }
}

/// A taxonomy reads the frontmatter key that shares its id unless it names
/// another, so its defaults depend on that id: the conversion *is* the
/// `Default` impl it cannot have.
impl From<String> for TaxonomyConfig {
    fn from(id: String) -> Self {
        Self {
            key: id,
            // opt-in: term pages and their index are extra output
            listing: false,
            template: None,
            // un-paginated until asked, like a collection with no `paginate`
            paginate: None,
            prefix: "page".into(),
        }
    }
}

/// The conventional cache policy, applied when `deploy { s3 { cache } }` is
/// present. A year is the maximum `max-age` any cache honours, and `immutable`
/// stops a reload from revalidating a file that cannot have changed.
impl CacheControl {
    pub(super) const IMMUTABLE: &'static str = "public, max-age=31536000, immutable";
    pub(super) const DEFAULT: &'static str = "public, max-age=0, must-revalidate";
}
