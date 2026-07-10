//! Baudelaire site configuration.
//!
//! Parsed from `config.kdl`. See [`Config::parse`] and [`Config::default`].
//! Conventional defaults live in [`defaults`]. Profile overlay in [`profile`].

pub mod defaults;
mod dispatch;
pub mod parse;
pub mod profile;
mod value;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use kdl::KdlDocument;

pub use defaults::SortKey;

/// Top-level site configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Site title.
    pub site: Option<String>,
    /// Canonical base URL, e.g. `https://example.net`.
    pub url: Option<String>,
    /// Default language code.
    pub lang: String,
    /// Default author.
    pub author: Option<String>,
    /// Content source directory.
    pub content: PathBuf,
    /// Output (distribution) directory.
    pub dist: PathBuf,
    /// Static passthrough directory.
    pub assets: PathBuf,
    /// Layout / template directory.
    pub templates: PathBuf,
    /// Generate directory-per-page URLs (`foo.typ` → `foo/index.html`).
    pub clean: bool,
    /// Build future-dated posts.
    pub future: bool,
    /// Emit `sitemap.xml` (requires `url`).
    pub sitemap: bool,
    /// `robots.txt` generation.
    pub robots: RobotsConfig,
    /// `llms.txt` generation.
    pub llms: LlmsConfig,
    /// Draft handling.
    pub draft: DraftConfig,
    /// Internal link checking.
    pub links: LinkConfig,
    /// Syndication feeds.
    pub feed: FeedConfig,
    /// Client-side search indexes.
    pub search: SearchConfig,
    /// Typst `sys.inputs` entries.
    pub inputs: Vec<(String, String)>,
    /// Typst features to enable (e.g. `html`).
    pub features: Vec<String>,
    /// Collection overrides keyed by id.
    pub collections: Vec<(String, CollectionConfig)>,
    /// Taxonomy definitions.
    pub taxonomies: Vec<(String, TaxonomyConfig)>,
    /// HTML output options.
    pub html: HtmlConfig,
    /// Image handling (lazy loading, optimization).
    pub images: ImagesConfig,
    /// Asset pipeline options (minify, bundle, fingerprint).
    pub asset: AssetConfig,
    /// Cache options.
    pub cache: CacheConfig,
    /// External command hooks run around the build.
    pub hooks: HooksConfig,
    /// Dev server options.
    pub serve: ServeConfig,
    /// The active profile name, if one was applied (exposed to pages).
    pub profile: Option<String>,
    /// Named profile partials (raw KDL, applied over base in [`Config::with_profile`]).
    pub profiles: Vec<(String, KdlDocument)>,
}

impl Config {
    /// Root directory for all build scratch state — the cache today, any other
    /// generated intermediates tomorrow. Single source for the location so it
    /// stays consistent between defaults and `clean`.
    pub const SCRATCH: &'static str = ".baudelaire";

    /// Human-readable site label for CLI output.
    pub fn label(&self) -> &str {
        self.site.as_deref().unwrap_or("unnamed")
    }

    /// The file a URL path is written to under `dist`, honoring clean URLs.
    /// Single source for the URL→file mapping, shared by page output and
    /// redirect stubs.
    pub fn destination(&self, url: &str) -> PathBuf {
        if url == "/" {
            return self.dist.join("index.html");
        }
        let trimmed = url.trim_matches('/');
        if self.clean {
            self.dist.join(trimmed).join("index.html")
        } else {
            self.dist.join(format!("{trimmed}.html"))
        }
    }
}

/// Feeds every build-affecting setting into the hasher so a config change
/// invalidates the build cache (a permalink or template tweak can alter every
/// page). Destructuring means a newly added field fails to compile until it is
/// accounted for here — no field can be silently forgotten.
impl std::hash::Hash for Config {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            site,
            url,
            lang,
            author,
            content,
            dist,
            assets,
            templates,
            clean,
            future,
            sitemap,
            robots,
            llms,
            draft,
            links,
            feed,
            search,
            inputs,
            features,
            collections,
            taxonomies,
            html,
            images,
            asset,
            cache,
            hooks,
            // Dev-server settings (port, bind, open, watch) never affect
            // generated output, so they must not key the cache — otherwise a
            // `serve` on a custom port would invalidate a `build`'s cache.
            serve: _,
            profile,
            // Raw, unapplied profile partials. Excluded deliberately: only the
            // *resolved* config drives the build, and applying a profile mutates
            // the fields above — so any effective change is already captured.
            profiles: _,
        } = self;
        (site, url, lang, author, content, dist, assets, templates).hash(state);
        (clean, future, sitemap, robots, llms, draft, links, feed, search).hash(state);
        (inputs, features, collections, taxonomies, html, images, asset, cache, hooks, profile)
            .hash(state);
    }
}

/// Per-collection override.
#[derive(Debug, Clone, Hash)]
pub struct CollectionConfig {
    /// Glob selecting members. `None` = convention (top-level dir under `content/`).
    pub glob: Option<String>,
    /// Sort key.
    pub sort: SortKey,
    /// Reverse sort order.
    pub reverse: bool,
    /// Permalink template, e.g. `/posts/{slug}/`.
    pub permalink: Option<String>,
    /// Default template file for pages in this collection.
    pub template: Option<String>,
    /// Items per generated index page. `None` = no pagination.
    pub paginate: Option<usize>,
    /// Template for the generated paginated index pages.
    pub list: Option<String>,
}

/// Taxonomy definition.
#[derive(Debug, Clone, Hash)]
pub struct TaxonomyConfig {
    /// Grouping structure.
    pub kind: TaxoKind,
    /// Frontmatter key to read terms from.
    pub key: String,
    /// Auto-generate index pages for each term.
    pub index: bool,
    /// Template for the generated taxonomy index + term pages.
    pub template: Option<String>,
}

/// Draft handling: whether drafts build, and the file-stem suffix marking one.
#[derive(Debug, Clone, Hash)]
pub struct DraftConfig {
    /// Build draft pages. Runtime flag, set by `--drafts` or a profile.
    pub build: bool,
    /// Suffix marking draft sources, e.g. `post.draft.typ`.
    pub suffix: String,
}

/// Internal link checking.
#[derive(Debug, Clone, Hash)]
pub struct LinkConfig {
    /// Treat unresolved internal `.typ` links as errors (else warnings).
    pub strict: bool,
}

/// Syndication feeds.
#[derive(Debug, Clone, Hash)]
pub struct FeedConfig {
    /// Formats to emit (requires `url`).
    pub formats: Vec<FeedKind>,
    /// Maximum items in a feed.
    pub limit: usize,
}

/// A syndication feed format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeedKind {
    Rss,
    Atom,
}

impl FeedKind {
    /// The conventional output file name for this format.
    pub fn file(self) -> &'static str {
        match self {
            Self::Rss => "rss.xml",
            Self::Atom => "atom.xml",
        }
    }
}

/// Client-side search index generation. Empty `formats` disables search.
#[derive(Debug, Clone, Hash)]
pub struct SearchConfig {
    /// Index formats to emit. Empty = disabled.
    pub formats: Vec<SearchFormat>,
    /// Page fields included in each indexed document.
    pub fields: Vec<SearchField>,
    /// Tokens excluded from the inverted index.
    pub stopwords: Vec<String>,
    /// Minimum token length kept in the inverted index.
    pub min_length: usize,
    /// Also emit a tiny JavaScript client next to each index.
    pub client: bool,
}

/// A client-side search index format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchFormat {
    /// A flat document list (`search.json`) — pair with any client library
    /// (Fuse.js, MiniSearch, …), which builds its own index at runtime.
    Json,
    /// A prebuilt inverted index (`search-index.json`): server-side tokenized
    /// so the client looks up terms directly instead of scanning every doc.
    Inverted,
}

impl SearchFormat {
    /// The conventional output file name for this format's index.
    pub fn file(self) -> &'static str {
        match self {
            Self::Json => "search.json",
            Self::Inverted => "search.inverted.json",
        }
    }

    /// The file name for this format's generated JavaScript client.
    pub fn client_file(self) -> &'static str {
        match self {
            Self::Json => "search.js",
            Self::Inverted => "search.inverted.js",
        }
    }
}

/// A page field selectable for indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchField {
    Title,
    Body,
    Tags,
}

/// Taxonomy grouping structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaxoKind {
    /// Flat list of terms (e.g. tags).
    List,
    /// Hierarchical / nested terms (e.g. series).
    Tree,
}

/// HTML output options.
#[derive(Debug, Clone, Hash)]
pub struct HtmlConfig {
    /// Pretty-print HTML.
    pub pretty: bool,
    /// Inline local assets (`/assets/…` refs) as `data:` URIs.
    pub embed: bool,
    /// Inject SEO + social meta tags (description, OpenGraph, Twitter, canonical)
    /// into each page's `<head>` from frontmatter and config.
    pub meta: bool,
}

/// Image handling: markup annotations and build-time optimization. Grouped so
/// every image setting lives in one `images { … }` block.
#[derive(Debug, Clone, Hash)]
pub struct ImagesConfig {
    /// Add `loading="lazy"` and `decoding="async"` to `<img>` elements.
    pub lazy: bool,
    /// Per-format build-time optimization.
    pub optimize: OptimizeConfig,
}

/// Build-time image optimization, per format. A format is enabled by naming it
/// in the `optimize { … }` block (`png`, `jpeg`); an absent format is left
/// untouched. Each format carries its own tuning.
#[derive(Debug, Clone, Hash, Default)]
pub struct OptimizeConfig {
    /// PNG optimization (oxipng), when enabled.
    pub png: Option<PngConfig>,
    /// JPEG optimization (re-encode), when enabled.
    pub jpeg: Option<JpegConfig>,
}

/// A raster format the optimizer recognizes, resolved from a file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl OptimizeConfig {
    /// Whether any format is enabled.
    pub fn any(&self) -> bool {
        self.png.is_some() || self.jpeg.is_some()
    }

    /// The enabled format for a file extension, matched leniently (`jpg`, `jpeg`,
    /// `jpe` all map to JPEG). `None` when unrecognized or that format is off.
    pub fn format(&self, ext: &str) -> Option<ImageFormat> {
        let matched = match ext.to_ascii_lowercase().as_str() {
            "png" => ImageFormat::Png,
            "jpg" | "jpeg" | "jpe" | "jfif" => ImageFormat::Jpeg,
            _ => return None,
        };
        let on = match matched {
            ImageFormat::Png => self.png.is_some(),
            ImageFormat::Jpeg => self.jpeg.is_some(),
        };
        on.then_some(matched)
    }
}

/// PNG optimization tuning (oxipng).
#[derive(Debug, Clone, Hash)]
pub struct PngConfig {
    /// Optimization preset, `0` (fast) – `6` (exhaustive).
    pub level: u8,
    /// Which ancillary chunks to strip.
    pub strip: PngStrip,
}

/// PNG ancillary-chunk stripping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PngStrip {
    /// Keep every chunk.
    None,
    /// Strip everything but display-affecting chunks (the default).
    Safe,
    /// Strip all non-critical chunks.
    All,
}

/// JPEG optimization tuning (re-encode).
#[derive(Debug, Clone, Hash)]
pub struct JpegConfig {
    /// Re-encode quality, `1`–`100`.
    pub quality: u8,
}

/// `robots.txt` generation. Enabled by the presence of a `robots` block.
#[derive(Debug, Clone, Hash, Default)]
pub struct RobotsConfig {
    /// Whether to emit `robots.txt`.
    pub enabled: bool,
    /// Paths disallowed for all crawlers. Empty = allow everything.
    pub disallow: Vec<String>,
}

/// `llms.txt` generation ([llmstxt.org]): a Markdown index of the site's pages
/// for LLM consumption. Enabled by the presence of an `llms` block.
///
/// [llmstxt.org]: https://llmstxt.org
#[derive(Debug, Clone, Hash, Default)]
pub struct LlmsConfig {
    /// Whether to emit `llms.txt`.
    pub enabled: bool,
    /// Optional one-line summary rendered as the blockquote under the title.
    pub summary: Option<String>,
}

/// Asset pipeline options. All opt-in — a fresh site copies assets verbatim.
///
/// CSS is minified with lightningcss; JavaScript is bundled and minified with
/// rolldown (the oxc-based bundler). Note the coupling: JavaScript is only
/// processed (bundled *and* minified) when [`AssetConfig::bundle`] is set — a
/// bundler owns the whole JS step. CSS minification is independent of bundling.
#[derive(Debug, Clone, Hash, Default)]
pub struct AssetConfig {
    /// Minify CSS (lightningcss) and, when bundling, JavaScript (rolldown).
    pub minify: bool,
    /// Bundle JavaScript entry points through rolldown (resolves imports and
    /// tree-shakes). Required for any JavaScript processing.
    pub bundle: bool,
    /// Content-hash asset filenames (`style.css` → `style.<hash>.css`) and
    /// rewrite references, for far-future caching.
    pub fingerprint: bool,
}

/// Cache options.
#[derive(Debug, Clone, Hash)]
pub struct CacheConfig {
    /// Cache directory.
    pub dir: PathBuf,
    /// Enable incremental builds.
    pub incremental: bool,
}

/// External command hooks. Each command runs through the system shell in the
/// project root — the escape hatch for tools baudelaire does not embed
/// (Tailwind, PostCSS, Pagefind, image optimizers, deploy scripts).
#[derive(Debug, Clone, Hash, Default)]
pub struct HooksConfig {
    /// Commands run before the build (before the asset pipeline), so any files
    /// they generate into `assets/` are picked up and fingerprinted.
    pub before: Vec<String>,
    /// Commands run after the site is written to `dist`.
    pub after: Vec<String>,
}

/// Dev server options.
#[derive(Debug, Clone, Hash)]
pub struct ServeConfig {
    /// Port to listen on.
    pub port: u16,
    /// Address to bind.
    pub bind: String,
    /// Open browser on start.
    pub open: bool,
    /// Watch for changes and rebuild.
    pub watch: bool,
    /// Extra paths to watch, beyond content, templates, and assets (e.g. a data
    /// directory or a Tailwind input outside `assets/`).
    pub include: Vec<String>,
    /// Paths the watcher ignores (e.g. hook-generated files), so a `before`
    /// hook writing into a watched directory does not trigger a rebuild loop.
    /// Checked first, so it overrides both the defaults and `include`.
    pub exclude: Vec<String>,
}
