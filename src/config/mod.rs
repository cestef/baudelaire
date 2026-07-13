//! Baudelaire site configuration.
//!
//! Parsed from `config.kdl`. See [`Config::parse`] and [`Config::default`].
//! Conventional defaults live in [`defaults`]. Profile overlay in [`profile`].

pub mod defaults;
pub(crate) mod dispatch;
pub mod parse;
pub mod profile;
#[cfg(test)]
mod tests;
mod value;

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
    /// Bundle index basename. A content file with this stem takes its slug from
    /// its parent directory instead of its filename, so `posts/hello/index.typ`
    /// becomes `/posts/hello/` (the "page bundle" layout, with colocated
    /// resources). `None` disables it — every page is keyed by its filename.
    pub index: Option<String>,
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
    /// Publishing destinations for the built site.
    pub publish: PublishConfig,
    /// Dev server options.
    pub serve: ServeConfig,
    /// The active profile name, if one was applied (exposed to pages).
    pub profile: Option<String>,
    /// Named profile partials (raw KDL, applied over base in [`Config::with_profile`]).
    pub profiles: Vec<(String, KdlDocument)>,
    /// The raw `config.kdl` text this config was parsed from. Profile overlay
    /// errors are reported against it — the retained profile nodes carry spans
    /// into this exact string.
    pub(crate) source: String,
}

impl Config {
    /// Root of all machine-local, regenerable build state, one subdirectory per
    /// subsystem:
    ///
    /// ```text
    /// .baudelaire/
    ///   cache/    incremental build cache — loss forces a full rebuild
    ///   publish/  per-backend publish skip-cache — loss forces idempotent re-sends
    /// ```
    ///
    /// Everything here is derivable, never authored: it is gitignored, wiped by
    /// `clean`, and safe to delete at any time. Single source for the location so
    /// defaults, `clean`, and each subsystem agree; join a subdir via [`scratch`].
    ///
    /// [`scratch`]: Config::scratch
    pub const SCRATCH: &'static str = ".baudelaire";

    /// The not-found page's output file. Flat at the dist root — the name
    /// static hosts serve for unmatched URLs — and what the dev server falls
    /// back to; single source for both.
    pub const NOT_FOUND: &'static str = "404.html";

    /// The path of a named scratch subdirectory (e.g. `cache`, `publish`) — the
    /// one builder every subsystem uses to locate its local state under
    /// [`SCRATCH`](Config::SCRATCH).
    pub fn scratch(sub: &str) -> PathBuf {
        PathBuf::from(Self::SCRATCH).join(sub)
    }

    /// Human-readable site label for CLI output.
    pub fn label(&self) -> &str {
        self.site.as_deref().unwrap_or("unnamed")
    }

    /// The configured base URL, normalized for joining. `None` when `url` is
    /// unset — URL-absolute features gate on this.
    pub fn base(&self) -> Option<BaseUrl> {
        self.url
            .as_deref()
            .map(|url| BaseUrl(url.trim_end_matches('/').to_owned()))
    }

    /// Look up a collection override by id.
    pub fn collection(&self, id: &str) -> Option<&CollectionConfig> {
        self.collections
            .iter()
            .find(|(n, _)| n == id)
            .map(|(_, c)| c)
    }

    /// The served name of the assets directory — its final path segment, and
    /// the leading segment of every asset URL. The single derivation shared by
    /// the asset pipeline and the embed transform.
    pub fn asset_name(&self) -> &str {
        self.assets
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("assets")
    }

    /// The processed assets directory under `dist` — where the pipeline writes
    /// and the embed transform reads.
    pub fn asset_dist(&self) -> PathBuf {
        self.dist.join(self.asset_name())
    }

    /// The file a URL path is written to under `dist`, honoring clean URLs.
    /// Single source for the URL→file mapping, shared by page output and
    /// redirect stubs.
    ///
    /// `..` segments are dropped here: permalink *templates* are already
    /// rejected at config parse, and this filter owns the defense for every
    /// other URL source (e.g. a frontmatter slug), so no page can ever be
    /// written outside `dist`.
    pub fn destination(&self, url: &str) -> PathBuf {
        if url == "/" {
            return self.dist.join("index.html");
        }
        let trimmed = url
            .split('/')
            .filter(|segment| !segment.is_empty() && *segment != "..")
            .collect::<Vec<_>>()
            .join("/");
        // 404 must be a flat `404.html`; under clean URLs a `404/` dir isn't served as not-found
        if trimmed == "404" {
            return self.dist.join(Self::NOT_FOUND);
        }
        if self.clean {
            self.dist.join(&trimmed).join("index.html")
        } else {
            self.dist.join(format!("{trimmed}.html"))
        }
    }
}

/// The site base URL with its trailing slash normalized away — the single
/// join rule for every consumer that makes root-relative paths absolute
/// (sitemap, feeds, robots, llms, meta tags).
#[derive(Debug, Clone)]
pub struct BaseUrl(String);

impl BaseUrl {
    /// Absolute URL for a root-relative path (a permalink or `/file`).
    pub fn join(&self, path: impl AsRef<str>) -> String {
        format!("{}{}", self.0, path.as_ref())
    }

    /// Absolute URL for a bare output file name sitting at the site root, e.g.
    /// `sitemap.xml` → `https://site/sitemap.xml`.
    pub fn file(&self, name: &str) -> String {
        self.join(format!("/{name}"))
    }

    /// Make a root-relative `path` absolute when a base is configured, else
    /// leave it as-is — the one "absolutize if we can, otherwise stay relative"
    /// rule shared by every URL emitter. Non-root-relative refs (external URLs)
    /// pass through untouched.
    pub fn resolve(base: Option<&BaseUrl>, path: &str) -> String {
        match base {
            Some(base) if path.starts_with('/') => base.join(path),
            _ => path.to_owned(),
        }
    }

    /// The site home page URL (base with a trailing slash).
    pub fn home(&self) -> String {
        format!("{}/", self.0)
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
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
            index,
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
            publish,
            // dev-server settings never affect output, so they must not key the cache —
            // else `serve` on a custom port would invalidate a `build`'s cache
            serve: _,
            profile,
            // raw unapplied partials: only the resolved config drives the build, and
            // applying a profile mutates the fields above, so any change is already captured
            profiles: _,
            // raw config text, kept only for error spans; a comment-only edit must not bust the cache
            source: _,
        } = self;
        (
            site, url, lang, author, content, index, dist, assets, templates,
        )
            .hash(state);
        (
            clean, future, sitemap, robots, llms, draft, links, feed, search,
        )
            .hash(state);
        (inputs, features, collections, taxonomies, html, images).hash(state);
        (asset, cache, hooks, publish, profile).hash(state);
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
    /// Permalink of the paginated index's first page. `None` = `/{id}/`; set to
    /// `/` to mount a collection's index at the site root (a blog home).
    pub index: Option<String>,
}

/// Taxonomy definition.
#[derive(Debug, Clone, Hash)]
pub struct TaxonomyConfig {
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
    Json,
}

impl FeedKind {
    /// The conventional output file name for this format.
    pub fn file(self) -> &'static str {
        match self {
            Self::Rss => "rss.xml",
            Self::Atom => "atom.xml",
            Self::Json => "feed.json",
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
    /// A prebuilt inverted index (`search.inverted.json`): server-side tokenized
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
    /// Give every heading a slug `id` (when it lacks one), so sections are
    /// deep-linkable and a table of contents can target them.
    pub anchors: bool,
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
/// CSS is minified with lightningcss, independently of bundling. JavaScript is
/// only processed (bundled *and* minified, via rolldown) when
/// [`AssetConfig::bundle`] is set — the bundler owns the whole JS step.
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

/// Publishing destinations for the built site. Each backend is an optional
/// block under `publish { … }`; adding a destination is one field here plus one
/// backend in [`crate::publish`]. Secrets are never stored here — a backend
/// reads its credentials from the environment at publish time.
#[derive(Debug, Clone, Hash, Default)]
pub struct PublishConfig {
    /// standard.site (AT Protocol) publishing.
    pub standard: Option<StandardConfig>,
}

/// The standard.site (AT Protocol) publishing target.
#[derive(Debug, Clone, Hash)]
pub struct StandardConfig {
    /// Account handle or DID to authenticate as, e.g. `you.bsky.social`.
    pub handle: String,
    /// Repository DID (a stable public identifier, not a secret). When set, the
    /// build emits the standard.site verification artifacts — the `.well-known`
    /// file and per-page `<link>` tags — offline; publishing checks it against
    /// the authenticated session.
    pub did: Option<String>,
    /// PDS/entryway host to authenticate and write records against.
    pub pds: String,
    /// Opt the publication into discovery surfaces.
    pub discover: bool,
    /// Publication icon, a path (under the project root) uploaded as a blob.
    pub icon: Option<PathBuf>,
    /// Which build-time verification artifacts to emit (requires `did`).
    pub verify: VerifyConfig,
}

/// The standard.site domain-verification artifacts the build emits, each
/// toggleable. Both require a configured `did`; either alone proves the site and
/// the records belong together, so a site may emit one, the other, or both.
#[derive(Debug, Clone, Hash)]
pub struct VerifyConfig {
    /// Emit `/.well-known/site.standard.publication` (the publication `at://` URI).
    pub wellknown: bool,
    /// Inject a per-page `<link rel="site.standard.document">` into dated pages.
    pub links: bool,
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
