//! Baudelaire site configuration.
//!
//! Parsed from `config.kdl`. See [`Config::parse`] and [`Config::default`].
//! Conventional defaults live in [`defaults`]. Profile overlay in [`profile`].

pub mod defaults;
pub(crate) mod dispatch;
mod node;
pub mod parse;
pub mod permalink;
pub mod profile;
#[cfg(test)]
mod tests;
mod url;
mod value;

use std::path::{Path, PathBuf};

use kdl::KdlDocument;

use crate::config::dispatch::Section;
use crate::error::{ConfigError, Result};
use crate::mime::ImageFormat;

pub use permalink::{Permalink, PermalinkCtx, PermalinkError};
pub use url::{BaseUrl, Percent, UrlStyle};

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
    /// The project root: what every other path is relative to, and what typst
    /// resolves `/`-absolute imports against. Explicit rather than inferred from
    /// `content`'s parent, which is only the root when `content` sits directly
    /// under it.
    pub root: PathBuf,
    /// Directory layout: where each kind of source lives, and where the build
    /// lands.
    pub paths: Paths,
    /// A theme package supplying templates, assets, and config defaults, named
    /// like any Typst dependency (`@preview/plume:1.0.0`). Everything it
    /// provides is a default the project overrides.
    pub theme: Option<String>,
    /// What the content tree contains and how it is read: bundles, drafts,
    /// future dating, collections, taxonomies.
    pub content: ContentConfig,
    /// Declared languages keyed by code, for a multi-language site. Empty means
    /// a single-language site (only `lang`); a non-empty block turns on i18n:
    /// filename suffixes (`post.fr.typ`) are recognized, non-default languages
    /// get a `/{code}` URL prefix, and each language can carry a display name,
    /// text direction, and a UI-string table. The default `lang` is always a
    /// known language whether or not it appears here.
    pub languages: Vec<(String, LanguageConfig)>,
    /// Asset pipeline options (minify, bundle, fingerprint, images).
    pub assets: AssetConfig,
    /// HTML output options.
    pub html: HtmlConfig,
    /// Link shape and link checking.
    pub links: LinkConfig,
    /// Files the build generates beside the pages: sitemap, robots, llms,
    /// feeds, search indexes, social cards.
    pub generate: GenerateConfig,
    /// How a visitor moves between the built pages: SPA runtime, single-file
    /// export, browser speculation hints.
    pub navigation: NavigationConfig,
    /// Remove orphaned outputs from `dist` on each build: files a previous
    /// build wrote that this one no longer produces (a deleted page, a renamed
    /// permalink, a dropped taxonomy term). The asset tree and build cache are
    /// never touched. Set with `prune #true | #false`.
    pub prune: bool,
    /// Typst engine knobs (`sys.inputs`, experimental features).
    pub typst: TypstConfig,
    /// Build-time constants exposed to client JS through the `baudelaire:config`
    /// virtual module: arbitrary scalars keyed by name.
    pub client: Vec<(String, crate::codegen::Value)>,
    /// Cache options.
    pub cache: CacheConfig,
    /// External command hooks run around the build.
    pub hooks: HooksConfig,
    /// Announce destinations for the built site.
    pub announce: AnnounceConfig,
    /// Deploy destinations for the built files.
    pub deploy: DeployConfig,
    /// Dev server options.
    pub serve: ServeConfig,
    /// The active profile name, if one was applied (exposed to pages).
    pub profile: Option<String>,
    /// Named profile partials (raw KDL, applied over base in [`Config::with_profile`]).
    pub profiles: Vec<(String, KdlDocument)>,
    /// The raw `config.kdl` text this config was parsed from. Profile overlay
    /// errors are reported against it: the retained profile nodes carry spans
    /// into this exact string.
    pub(crate) source: String,
}

/// Directory layout, every entry relative to [`Config::root`].
#[derive(Debug, Clone, Hash)]
pub struct Paths {
    /// Content source directory.
    pub content: PathBuf,
    /// Output (distribution) directory.
    pub dist: PathBuf,
    /// Asset pipeline source directory (minified, bundled, fingerprinted).
    pub assets: PathBuf,
    /// Static passthrough directory: copied verbatim to the `dist` root, with no
    /// processing, no fingerprint, no URL prefix.
    pub r#static: PathBuf,
    /// Layout / template directory.
    pub templates: PathBuf,
}

impl Paths {
    /// Every configured directory the build *reads*, paired with the key that
    /// names it. The single list of what [`dist`](Paths::dist) must stay clear
    /// of, walked by both the containment guard ([`swallowed`]) and the prune
    /// sweep, so a new `paths` entry is covered by adding it here alone.
    ///
    /// [`swallowed`]: Paths::swallowed
    pub fn sources(&self) -> [(&'static str, &Path); 4] {
        [
            ("content", &self.content),
            ("assets", &self.assets),
            ("static", &self.r#static),
            ("templates", &self.templates),
        ]
    }

    /// The first source directory `dist` would contain, if any.
    ///
    /// The prune sweep deletes everything under `dist` the build did not write,
    /// so a `dist` holding the sources deletes the sources: `paths { dist "." }`
    /// took `config.kdl` and the whole content tree with it, and reported a
    /// successful build. Refusing the config is the only place this can be
    /// caught, since by the time the sweep runs every path looks alike.
    ///
    /// Entries resolve against `root` rather than the process cwd, so a caller
    /// that has not changed into the project still gets the right answer.
    pub fn swallowed(&self, root: &Path) -> Option<(&'static str, &Path)> {
        let dist = crate::fs::resolved(root.join(&self.dist));
        self.sources()
            .into_iter()
            .find(|(_, path)| crate::fs::resolved(root.join(path)).starts_with(&dist))
    }
}

/// What the content tree holds and how it is read. The directory itself is
/// [`Paths::content`]; everything here is about the pages inside it.
#[derive(Debug, Clone, Hash)]
pub struct ContentConfig {
    /// Bundle index basename. A content file with this stem takes its slug from
    /// its parent directory instead of its filename, so `posts/hello/index.typ`
    /// becomes `/posts/hello/` (the "page bundle" layout, with colocated
    /// resources). `None` disables it: every page is keyed by its filename.
    pub index: Option<String>,
    /// Build future-dated posts.
    pub future: bool,
    /// Draft handling.
    pub draft: DraftConfig,
    /// Collection overrides keyed by id.
    pub collections: Vec<(String, CollectionConfig)>,
    /// Taxonomy definitions.
    pub taxonomies: Vec<(String, TaxonomyConfig)>,
}

/// The files a build emits beside the pages themselves. Each one is opt-in:
/// either a flag or a block whose presence turns it on.
#[derive(Debug, Clone, Hash, Default)]
pub struct GenerateConfig {
    /// Emit `sitemap.xml`. Opt-in like its neighbours, and needs a `url`.
    pub sitemap: bool,
    /// `robots.txt` generation.
    pub robots: RobotsConfig,
    /// `llms.txt` generation.
    pub llms: LlmsConfig,
    /// Syndication feeds.
    pub feed: FeedConfig,
    /// Client-side search indexes.
    pub search: SearchConfig,
    /// Generated social cards.
    pub cards: CardsConfig,
}

/// How a visitor moves between the built pages. Three independent strategies,
/// each enabled by the presence of its block.
#[derive(Debug, Clone, Hash, Default)]
pub struct NavigationConfig {
    /// Client-side navigation between the built pages.
    pub spa: SpaConfig,
    /// Single-file (standalone) HTML export.
    pub standalone: StandaloneConfig,
    /// Browser-native prefetch/prerender hints.
    pub speculation: SpeculationConfig,
}

/// Typst engine knobs.
#[derive(Debug, Clone, Hash, Default)]
pub struct TypstConfig {
    /// Extra experimental Typst features to enable (e.g. `a11y-extras`). `html`
    /// is always forced on in `world.rs`, so this list is purely additive and
    /// never needs to include it.
    pub features: Vec<String>,
    /// Typst `sys.inputs` entries.
    pub inputs: Vec<(String, String)>,
}

impl Config {
    /// Parse a site's config, layered over whatever defaults its theme supplies.
    ///
    /// Two passes, because the config is what names the theme: the site's own
    /// text is read once to learn that, then re-applied over the theme's
    /// `theme.kdl` so every key the site states wins and every key it leaves out
    /// falls back. A site with no theme parses exactly once.
    ///
    /// `root` is the project directory a directory-theme is resolved against,
    /// passed rather than taken from the process cwd so a caller that has not
    /// changed into the project (a test, an embedding) resolves correctly.
    pub fn load(text: &str, root: &std::path::Path) -> Result<Self> {
        let config = Self::parse(text)?;
        let Some(spec) = config.theme.as_deref() else {
            return Ok(config);
        };
        let theme = crate::theme::Theme::resolve(spec, root)?;
        let Some(defaults) = theme.config() else {
            return Ok(config);
        };
        let base = Self::parse(&crate::fs::read_to_string(&defaults)?)?;
        Self::parse_over(base, text)
    }

    /// Apply a config text over an existing config, rather than over the
    /// built-in defaults: how a theme's `theme.kdl` becomes the floor the site's
    /// own config stands on.
    fn parse_over(base: Self, text: &str) -> Result<Self> {
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        // The site's text, not the theme's: every span a later error points at
        // has to land in the file the author is editing.
        let mut config = Self {
            source: text.to_owned(),
            ..base
        };
        config.apply(doc.nodes(), text)?;
        Ok(config)
    }

    /// Root of all machine-local, regenerable build state, one subdirectory per
    /// subsystem:
    ///
    /// ```text
    /// .baudelaire/
    ///   cache/    incremental build cache: loss forces a full rebuild
    ///   announce/  per-backend announce skip-cache: loss forces idempotent re-sends
    /// ```
    ///
    /// Everything here is derivable, never authored: it is gitignored, wiped by
    /// `clean`, and safe to delete at any time. Single source for the location so
    /// defaults, `clean`, and each subsystem agree; join a subdir via [`scratch`].
    ///
    /// [`scratch`]: Config::scratch
    pub const SCRATCH: &'static str = ".baudelaire";

    /// The not-found page's output file. Flat at the dist root, the name
    /// static hosts serve for unmatched URLs, and what the dev server falls
    /// back to; single source for both.
    pub const NOT_FOUND: &'static str = "404.html";

    /// The key holding the profile partials, shared by the top-level rule that
    /// parses it and the guard refusing one *inside* a profile.
    pub(crate) const PROFILES: &'static str = "profiles";

    /// The path of a named scratch subdirectory (e.g. `cache`, `announce`): the
    /// one builder every subsystem uses to locate its local state under
    /// [`SCRATCH`](Config::SCRATCH).
    pub fn scratch(sub: &str) -> PathBuf {
        PathBuf::from(Self::SCRATCH).join(sub)
    }

    /// Human-readable site label for CLI output.
    pub fn label(&self) -> &str {
        self.site.as_deref().unwrap_or("unnamed")
    }

    /// The site title in a given language: the language's `site` override if it
    /// has one, else the site-wide title. Used for per-language feed titles.
    pub fn title(&self, code: &str) -> &str {
        self.language(code)
            .and_then(|lang| lang.site.as_deref())
            .unwrap_or_else(|| self.label())
    }

    /// The author in a given language: the language's `author` override if it
    /// has one, else the site-wide author.
    pub fn author(&self, code: &str) -> Option<&str> {
        self.language(code)
            .and_then(|lang| lang.author.as_deref())
            .or(self.author.as_deref())
    }

    /// A language's display name, if declared (e.g. `Français`), else `None`.
    pub fn name(&self, code: &str) -> Option<&str> {
        self.language(code).and_then(|lang| lang.name.as_deref())
    }

    /// The writing direction (`rtl`) declared for a language, if any; `None`
    /// means the default `ltr`.
    pub fn dir(&self, code: &str) -> Option<&str> {
        self.language(code)
            .and_then(|lang| lang.dir.as_deref())
            .or_else(|| Rtl::of(code))
    }

    /// A language's UI-string table (empty when it declares none), exposed to
    /// templates as `page.strings` and to client JS via `baudelaire:i18n`.
    pub fn strings(&self, code: &str) -> &[(String, crate::codegen::Value)] {
        self.language(code).map_or(&[], |lang| &lang.strings)
    }

    /// The declared config for a language code, if any.
    fn language(&self, code: &str) -> Option<&LanguageConfig> {
        self.languages
            .iter()
            .find(|(id, _)| id == code)
            .map(|(_, lang)| lang)
    }

    /// The configured base URL, normalized for joining. `None` when `url` is
    /// unset: URL-absolute features gate on this.
    pub fn base(&self) -> Option<BaseUrl> {
        self.url.as_deref().map(BaseUrl::new)
    }

    /// The path the site is served under, from the `url`'s path component
    /// (`url "https://host/docs"` -> `/docs`); empty for a root-hosted site.
    /// Every on-page root-absolute URL is prefixed with it so the site works
    /// under a subdirectory, leaving the on-disk layout unchanged.
    pub fn base_path(&self) -> &str {
        self.url.as_deref().map_or("", |url| {
            let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
            rest.find('/')
                .map_or("", |slash| rest[slash..].trim_end_matches('/'))
        })
    }

    /// Prefix a root-absolute site path with the [`base_path`](Self::base_path).
    /// Protocol-relative (`//`) and non-root refs pass through untouched.
    pub fn prefixed(&self, path: &str) -> String {
        match self.base_path() {
            "" => path.to_owned(),
            base if path.starts_with('/') && !path.starts_with("//") => format!("{base}{path}"),
            _ => path.to_owned(),
        }
    }

    /// The DID a `standard.site` verification artifact should reference, present
    /// only when the backend is configured *with* a `did` and the artifact's
    /// `verify` flag is on; `artifact` selects that flag (e.g. `|v| v.links`).
    /// The single gate the render transform and the well-known processor share,
    /// so both agree on when an artifact is emitted and neither re-checks the
    /// `did` after gating.
    #[cfg(feature = "announce")]
    pub(crate) fn verify_did(&self, artifact: impl Fn(&VerifyConfig) -> bool) -> Option<&str> {
        let standard = self.announce.standard.as_ref()?;
        artifact(&standard.verify)
            .then_some(standard.did.as_deref())
            .flatten()
    }

    /// Look up a collection override by id.
    pub fn collection(&self, id: &str) -> Option<&CollectionConfig> {
        self.content
            .collections
            .iter()
            .find(|(n, _)| n == id)
            .map(|(_, c)| c)
    }

    /// The served name of the assets directory: its final path segment, and
    /// the leading segment of every asset URL. The single derivation shared by
    /// the asset pipeline and the embed transform.
    pub fn asset_name(&self) -> &str {
        self.paths
            .assets
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("assets")
    }

    /// The URL prefix every processed asset is served under. The single source
    /// for it: the pipeline builds its map keys from this, and the render layer
    /// decides from it whether a reference could name an asset at all.
    pub fn asset_prefix(&self) -> String {
        format!("/{}", self.asset_name())
    }

    /// The processed assets directory under `dist`: the *published* location,
    /// read by the dev server and by whatever hosts `dist`.
    pub fn asset_dist(&self) -> PathBuf {
        self.paths.dist.join(self.asset_name())
    }

    /// Where the asset pipeline writes during a build, published over
    /// [`Config::asset_dist`] by a rename once every page is on disk, so a
    /// failed build leaves the served assets the existing HTML references.
    /// Everything reading processed assets mid-build reads here.
    pub fn asset_staging(&self) -> PathBuf {
        self.paths
            .dist
            .join(format!(".{}.staging", self.asset_name()))
    }

    /// The file a URL path is written to under `dist`, honoring clean URLs.
    /// Single source for the URL-to-file mapping, shared by page output and
    /// redirect stubs.
    ///
    /// `..` segments are dropped here: permalink *templates* are already
    /// rejected at config parse, and this filter owns the defense for every
    /// other URL source (e.g. a frontmatter slug), so no page can ever be
    /// written outside `dist`.
    pub fn destination(&self, url: &str) -> PathBuf {
        if url == "/" {
            return self.paths.dist.join("index.html");
        }
        let trimmed = url
            .split('/')
            .filter(|segment| !segment.is_empty() && *segment != "..")
            .collect::<Vec<_>>()
            .join("/");
        // 404 must be a flat `404.html`; under clean URLs a `404/` dir isn't
        // served as not-found. A translated `404.fr.typ` localizes to
        // `/{lang}/404/` and belongs at `{lang}/404.html` for the same reason.
        // Only a language scope counts: `/notes/404/` is an ordinary page.
        let stem = trimmed.strip_suffix(UrlStyle::PAGE).unwrap_or(&trimmed);
        // the not-found page's URL stem, derived so its name is written once
        let not_found = Self::NOT_FOUND
            .strip_suffix(UrlStyle::PAGE)
            .unwrap_or(Self::NOT_FOUND);
        if stem == not_found {
            return self.paths.dist.join(Self::NOT_FOUND);
        }
        if let Some(scope) = stem
            .strip_suffix(not_found)
            .and_then(|head| head.strip_suffix('/'))
            .filter(|scope| self.languages.iter().any(|(code, _)| code == scope))
        {
            return self.paths.dist.join(scope).join(Self::NOT_FOUND);
        }
        match self.links.style {
            UrlStyle::Clean => self.paths.dist.join(&trimmed).join("index.html"),
            // A flat page URL already names its file; a raw path (a frontmatter
            // `redirect` old-path) still needs the extension.
            UrlStyle::Flat => self
                .paths
                .dist
                .join(self.links.style.url(&trimmed).trim_start_matches('/')),
        }
    }

    /// Whether the site declares languages beyond the default.
    pub fn multilingual(&self) -> bool {
        !self.languages.is_empty()
    }

    /// Whether `code` is a language the site builds: a declared one, or the
    /// default `lang` (always known, listed or not).
    pub fn knows(&self, code: &str) -> bool {
        code == self.lang || self.languages.iter().any(|(id, _)| id == code)
    }

    /// Every language the site builds, default first then declared ones in
    /// config order (default deduplicated). A single-language site yields just
    /// the default. The single source for iterating languages.
    pub fn langs(&self) -> Vec<&str> {
        let declared = self.languages.iter().map(|(id, _)| id.as_str());
        std::iter::once(self.lang.as_str())
            .chain(declared.filter(|id| *id != self.lang))
            .collect()
    }

    /// A root-relative `path` under `code`: prefixed with `/{code}` for a
    /// non-default language, unchanged for the default (which keeps clean root
    /// URLs). The single localization rule for permalinks and every generator.
    pub fn localize(&self, code: &str, path: &str) -> String {
        match code == self.lang {
            true => path.to_owned(),
            false if path == "/" => format!("/{code}/"),
            false => format!("/{code}{path}"),
        }
    }

    /// The language path segment for `code`: empty for the default, the code
    /// otherwise. It prefixes a generated page's identity (its
    /// [`crate::content::PageId`] and virtual source path) and names a
    /// language's output subdirectory (feeds, sitemaps), so both mirror the
    /// localized URL. `id` is an optional trailing segment. Derived from
    /// [`Config::localize`], its single source.
    pub fn scope(&self, code: &str, id: &str) -> String {
        self.localize(code, &format!("/{id}"))
            .trim_matches('/')
            .to_owned()
    }
}

/// Feeds every build-affecting setting into the hasher so a config change
/// invalidates the build cache (a permalink or template tweak can alter every
/// page). Destructuring means a newly added field fails to compile until it is
/// accounted for here: no field can be silently forgotten.
impl std::hash::Hash for Config {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            // where the project sits, not what it builds: including it would
            // undo the portable manifest keys (`mv site site2` must still hit)
            root: _,
            site,
            url,
            lang,
            author,
            paths,
            theme,
            content,
            languages,
            assets,
            html,
            links,
            generate,
            navigation,
            prune,
            typst,
            client,
            cache,
            hooks,
            announce,
            deploy,
            // dev-server settings never affect output, so they must not key the cache;
            // else `serve` on a custom port would invalidate a `build`'s cache
            serve: _,
            profile,
            // raw unapplied partials: only the resolved config drives the build, and
            // applying a profile mutates the fields above, so any change is already captured
            profiles: _,
            // raw config text, kept only for error spans; a comment-only edit must not bust the cache
            source: _,
        } = self;
        (site, url, lang, author, paths, theme, content, languages).hash(state);
        (assets, html, links, generate, navigation, prune).hash(state);
        (typst, client, cache, hooks, announce, deploy, profile).hash(state);
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
    /// Where the collection's listing is served: the permalink of its first
    /// page. `None` = `/{id}/`; set to `/` to mount a blog at the site root.
    pub mount: Option<String>,
    /// Path segment before a paginated page number: `/{id}/{prefix}/{n}/`.
    /// Defaults to `page` (`/blog/page/2/`); an empty string drops the segment
    /// entirely (`/blog/2/`).
    pub prefix: String,
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

impl Named for SortKey {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("order", Self::Order),
        ("date", Self::Date),
        ("title", Self::Title),
    ];
}

/// Languages written right to left, so `dir="rtl"` is right without the site
/// having to say so. A monolingual `lang "ar"` site has no `languages` block to
/// declare `dir` in, and so could never get it.
struct Rtl;

impl Rtl {
    /// Primary subtags, and the script subtags that imply the direction
    /// whatever the language (`az-Arab`).
    const LANGS: &'static [&'static str] = &[
        "ar", "arc", "ckb", "dv", "fa", "he", "khw", "ks", "ps", "sd", "ug", "ur", "yi",
    ];
    const SCRIPTS: &'static [&'static str] = &["adlm", "arab", "hebr", "nkoo", "thaa"];

    fn of(code: &str) -> Option<&'static str> {
        let mut parts = code.split(['-', '_']).map(str::to_ascii_lowercase);
        let primary = parts.next()?;
        let rtl = Self::LANGS.contains(&primary.as_str())
            || parts.any(|part| Self::SCRIPTS.contains(&part.as_str()));
        rtl.then_some("rtl")
    }
}

/// One declared language in a multi-language site.
#[derive(Debug, Clone, Default, Hash, serde::Serialize)]
pub struct LanguageConfig {
    /// Display name for a language switcher, e.g. `Français`. Falls back to the
    /// code when unset.
    pub name: Option<String>,
    /// Writing direction, `ltr` (default) or `rtl`, surfaced as `<html dir>`.
    pub dir: Option<String>,
    /// Per-language site title override (else the site-wide `site`).
    pub site: Option<String>,
    /// Per-language author override (else the site-wide `author`).
    pub author: Option<String>,
    /// UI-string table for this language, exposed to templates as
    /// `page.strings` and to client JS via `baudelaire:i18n`.
    pub strings: Vec<(String, crate::codegen::Value)>,
}

/// Taxonomy definition.
#[derive(Debug, Clone, Hash)]
pub struct TaxonomyConfig {
    /// Frontmatter key to read terms from.
    pub key: String,
    /// Generate a page per term, plus one listing every term appears on.
    pub listing: bool,
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

/// Link shape and link checking: what a page's URL looks like, and how hard the
/// build tries to prove every reference to one resolves.
#[derive(Debug, Clone, Hash)]
pub struct LinkConfig {
    /// How permalinks map onto output files: clean (directory-per-page) or flat
    /// (`.html`). Set under `links { style "clean" | "flat" }`.
    pub style: UrlStyle,
    /// Treat unresolved internal `.typ` links as errors (else warnings).
    pub strict: bool,
    /// Also verify outbound `http(s)` links over the network.
    ///
    /// Read by `check` alone: a build stays offline and deterministic, so a
    /// flaky host or an airplane can never change what it produces. `check
    /// --external` turns it on for one run.
    pub external: bool,
}

/// Syndication feeds.
#[derive(Debug, Clone, Hash)]
pub struct FeedConfig {
    /// Formats to emit (requires `url`).
    pub formats: Vec<FeedKind>,
    /// Maximum items in a feed.
    pub limit: usize,
    /// Also emit a feed per taxonomy term, beside that term's listing page
    /// (`/tags/rust/rss.xml`), so a reader can follow one tag rather than the
    /// whole site. Follows the term pages, so it needs `index=#true` on the
    /// taxonomy.
    pub terms: bool,
}

/// A syndication feed format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeedKind {
    Rss,
    Atom,
    Json,
}

impl Named for FeedKind {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("rss", Self::Rss),
        ("atom", Self::Atom),
        ("json", Self::Json),
    ];
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

    /// The media type a `<link rel="alternate">` announces this format under,
    /// and how a reader tells the three apart when a page advertises several.
    /// Beside [`file`](Self::file) because a format's name and its type are the
    /// same fact, and an autodiscovery tag needs both.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Rss => "application/rss+xml",
            Self::Atom => "application/atom+xml",
            Self::Json => "application/feed+json",
        }
    }

    /// This feed's absolute URL under `base`, for a language `scope` (empty for
    /// the default language).
    ///
    /// One derivation, because two places name the same file and a reader would
    /// notice if they disagreed: the feed writes this into its own `<id>` and
    /// `feed_url`, and every page's `<head>` advertises it. Note the file name
    /// is appended to the scope's directory URL rather than joined as a path
    /// segment, which would give it a trailing slash.
    pub fn url(self, base: &BaseUrl, scope: &str) -> String {
        format!("{}{}", base.join(Permalink::join(&[scope])), self.file())
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
    /// A flat document list (`search.json`): pair with any client library
    /// (Fuse.js, MiniSearch, ..), which builds its own index at runtime.
    Json,
    /// A prebuilt inverted index (`search.inverted.json`): server-side tokenized
    /// so the client looks up terms directly instead of scanning every doc.
    Inverted,
}

impl Named for SearchFormat {
    const NAMES: &'static [(&'static str, Self)] =
        &[("json", Self::Json), ("inverted", Self::Inverted)];
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

impl Named for SearchField {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("title", Self::Title),
        ("body", Self::Body),
        ("tags", Self::Tags),
    ];
}

/// HTML output options.
#[derive(Debug, Clone, Hash)]
pub struct HtmlConfig {
    /// Pretty-print HTML.
    pub pretty: bool,
    /// Inline local assets (`/assets/..` refs) as `data:` URIs.
    pub embed: bool,
    /// Inject SEO + social meta tags (description, OpenGraph, Twitter, canonical)
    /// into each page's `<head>` from frontmatter and config.
    pub meta: bool,
    /// Give every heading a slug `id` (when it lacks one), so sections are
    /// deep-linkable and a table of contents can target them.
    pub anchors: bool,
    /// Rewrite syntax-highlight colours as CSS classes.
    pub highlight: HighlightConfig,
}

/// Turn typst's inline highlight colours into CSS classes, so a stylesheet owns
/// the palette and can follow a light/dark toggle.
///
/// typst-html bakes a highlight theme's colours into a `style="color: .."` on
/// every span, with no option to emit classes. A fixed colour cannot follow a
/// runtime theme switch, so the documented workaround was to author a `.tmTheme`
/// of *meaningless sentinel hex values* and remap each one with
/// `pre code [style*="e5d004"] { color: var(--kw) !important }`. That is what
/// this replaces.
#[derive(Debug, Clone, Hash, Default)]
pub struct HighlightConfig {
    /// Whether to rewrite at all; the block's presence turns it on.
    pub enabled: bool,
    /// Scope name to the colour the theme paints it, `keyword "#e5d004"`,
    /// mirroring the `.tmTheme`. A colour named here becomes `sx-<name>`;
    /// anything unnamed falls back to `sx-<hex>`, which still beats an
    /// attribute-substring selector.
    pub scopes: Vec<(String, String)>,
}

impl HighlightConfig {
    /// The class a highlight `colour` is rewritten to. The single naming rule,
    /// so the emitted markup and any generated stylesheet agree by construction.
    pub fn class(&self, colour: &str) -> String {
        let named = self
            .scopes
            .iter()
            .find(|(_, hex)| hex.eq_ignore_ascii_case(colour))
            .map(|(name, _)| name.as_str());
        format!("sx-{}", named.unwrap_or(colour.trim_start_matches('#')))
    }
}

/// Single-file export: the whole site inlined into one HTML document, each
/// page a route the bundled router swaps in. Enabled by the presence of a
/// `navigation { standalone { .. } }` block.
#[derive(Debug, Clone, Hash)]
pub struct StandaloneConfig {
    /// Whether to emit the single-file export.
    pub enabled: bool,
    /// Output file name, relative to `dist`.
    pub file: String,
    /// Permalink of the page whose `<head>` and body seed the shell: the route
    /// shown before any navigation, and the only one that renders without
    /// JavaScript. `None` means the site home (`/`, localized to `lang`).
    pub entry: Option<String>,
    /// How the router encodes the current route in the address bar.
    pub router: Router,
}

/// Generated social cards: the image a link to this site unfurls into, rendered
/// per page from a Typst template. Enabled by the presence of a
/// `generate { cards { .. } }` block.
///
/// The template is compiled to a *paged* document, not an HTML one, so it is
/// ordinary Typst: `html.elem` does not exist there, and page layout does.
#[derive(Debug, Clone, Hash)]
pub struct CardsConfig {
    /// Whether to render cards.
    pub enabled: bool,
    /// The template file under the templates directory.
    pub template: String,
    /// Card size in pixels. The card is one page rendered at one pixel per
    /// point, so these are also the page's dimensions in points.
    pub width: u32,
    pub height: u32,
}

impl CardsConfig {
    /// The directory cards are written to under `dist`, and the leading segment
    /// of every card URL.
    pub const DIR: &'static str = "cards";

    /// The widest and tallest a card may be. Unfurlers cap well below this; the
    /// limit exists so a typo cannot ask for a gigapixel rasterization.
    pub(crate) const MAX: i64 = 4096;

    /// The served URL of a page's card, whether or not it has been rendered
    /// yet: the meta transform names it while the file is still being made, the
    /// renderer writes it, and the prune keeps it, so all three have to derive
    /// it the same way.
    pub fn url(&self, permalink: &str) -> String {
        let stem = permalink.trim_matches('/');
        // A flat-URL site's permalink already names a file; `about.html.png`
        // would be an odd thing to serve.
        let stem = stem.strip_suffix(".html").unwrap_or(stem);
        match stem.is_empty() {
            // the home page, whose permalink is just `/`
            true => format!("/{}/index.png", Self::DIR),
            false => format!("/{}/{stem}.png", Self::DIR),
        }
    }

    /// Where that URL lands under `dist`.
    pub fn path(&self, dist: &std::path::Path, permalink: &str) -> PathBuf {
        dist.join(self.url(permalink).trim_start_matches('/'))
    }

    /// Whether cards are actually produced: configured *and* compiled in. A
    /// build without the `cards` feature has no rasterizer, so pointing pages at
    /// images it cannot make would be worse than making none.
    pub fn active(&self) -> bool {
        self.enabled && cfg!(feature = "cards")
    }
}

/// Browser-native navigation hints: a `<script type="speculationrules">` telling
/// the browser to fetch, or fully render, an internal link's target before it is
/// clicked. Enabled by the presence of a `navigation { speculation { .. } }`
/// block.
///
/// The zero-JavaScript neighbour of [`SpaConfig`]: the browser does the work, so
/// nothing has to be shipped, mounted, or maintained. Unsupported browsers
/// ignore the script.
#[derive(Debug, Clone, Hash)]
pub struct SpeculationConfig {
    /// Whether to emit the rules.
    pub enabled: bool,
    /// How eagerly to fetch a link's target (cheap: bytes only).
    pub prefetch: Eagerness,
    /// How eagerly to render it in full (expensive: a hidden page, its scripts
    /// running), so the click paints instantly.
    pub prerender: Eagerness,
}

/// How eagerly the browser should act on a speculation rule, from the API's own
/// scale, plus a [`Eagerness::None`] that emits no rule at all for that action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Eagerness {
    /// Emit no rule: this action is off.
    #[default]
    None,
    /// On pointer-down: the last moment before a navigation.
    Conservative,
    /// On hover, roughly, once intent looks real.
    Moderate,
    /// As soon as a link looks like a plausible next step.
    Eager,
    /// At once, for every matching link on the page.
    Immediate,
}

impl Named for Eagerness {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("none", Self::None),
        ("conservative", Self::Conservative),
        ("moderate", Self::Moderate),
        ("eager", Self::Eager),
        ("immediate", Self::Immediate),
    ];
}

/// Client-side navigation over the ordinary multi-file output: a runtime
/// intercepts internal link clicks, fetches the target page, and swaps one
/// container instead of reloading. Enabled by the presence of a
/// `navigation { spa { .. } }` block.
#[derive(Debug, Clone, Hash)]
pub struct SpaConfig {
    /// Whether to emit the navigation runtime.
    pub enabled: bool,
    /// CSS selector of the element swapped on navigation. Everything outside it
    /// (a header, a sidebar) survives untouched, so it must be the one element
    /// whose contents differ between pages.
    pub root: String,
    /// When to warm a link's target before it is clicked.
    pub prefetch: Prefetch,
}

/// How a router represents the active route in the URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Router {
    /// `#/blog/post/`: the only mode that survives `file://`, where a single
    /// file is normally opened, since it never asks the browser for a path the
    /// filesystem has to have.
    #[default]
    Hash,
    /// `/blog/post/`, through the History API. Needs the file served by a host
    /// that answers every route with it.
    History,
}

/// When the router warms a link's target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Prefetch {
    /// Never: every navigation pays its own fetch.
    None,
    /// On pointer-over or keyboard focus, the moment intent is visible.
    #[default]
    Hover,
    /// As soon as the link scrolls into view. Warms far more than is clicked.
    Visible,
}

/// An enum spelled out in config as one of a fixed set of names.
///
/// [`Named::NAMES`] is that set: config parsing maps through it, its
/// unknown-value suggestions are derived from it, and [`Named::name`] reads
/// back out of it. One table, so a variant can never parse under one spelling
/// and be generated under another.
pub trait Named: Copy + PartialEq + Sized + 'static {
    const NAMES: &'static [(&'static str, Self)];

    /// The name this variant is configured as, and the one generated code sees.
    fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(_, variant)| *variant == self)
            .map(|(name, _)| *name)
            .expect("NAMES lists every variant")
    }

    /// The variant a config name spells, if any: the read direction of
    /// [`NAMES`](Named::NAMES), and the only way a name becomes a variant.
    fn of(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, variant)| *variant)
    }
}

impl Named for Router {
    const NAMES: &'static [(&'static str, Self)] =
        &[("hash", Self::Hash), ("history", Self::History)];
}

impl Named for Prefetch {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("none", Self::None),
        ("hover", Self::Hover),
        ("visible", Self::Visible),
    ];
}

/// Image handling: markup annotations and build-time optimization. Grouped so
/// every image setting lives in one `assets { images { .. } }` block.
#[derive(Debug, Clone, Hash)]
pub struct ImagesConfig {
    /// Add `loading="lazy"` and `decoding="async"` to `<img>` elements.
    pub lazy: bool,
    /// Externalize typst-embedded images: write each `image()` to a file under
    /// the asset URL and reference it, instead of typst's default inline
    /// base64 `data:` URI. Forced off while `html.embed` is on (which re-inlines
    /// asset references), so the two never fight.
    pub extract: bool,
    /// Per-format build-time optimization.
    pub optimize: OptimizeConfig,
    /// Responsive width variants (`srcset`).
    pub responsive: ResponsiveConfig,
}

impl ImagesConfig {
    /// Whether to externalize typst-embedded images: the `extract` switch, unless
    /// `html.embed` is inlining everything (in which case externalizing would be
    /// undone immediately).
    pub fn externalize(&self, html: &HtmlConfig) -> bool {
        self.extract && !html.embed
    }
}

/// Responsive images: pre-generate downscaled copies of each raster and let the
/// browser pick the smallest that fits via `srcset`. Enabled by the presence of
/// a `responsive` block. Variants stay in the source format (a jpeg source
/// yields smaller jpegs); a width wider than the source is skipped, never
/// upscaled.
#[derive(Debug, Clone, Hash)]
pub struct ResponsiveConfig {
    /// Whether to emit width variants.
    pub enabled: bool,
    /// Target widths in CSS pixels. The source's own width is always the largest
    /// candidate, so these only add smaller sizes.
    pub widths: Vec<u32>,
    /// JPEG re-encode quality (`1`–`100`) for downscaled variants. PNG variants
    /// are re-encoded losslessly and ignore this.
    pub quality: u8,
    /// The `sizes` attribute for images the author left unsized: a media-query
    /// list describing the image's displayed width so the browser picks the
    /// smallest variant that fits (`(min-width: 60rem) 640px, 100vw`). `None`
    /// emits no attribute, which the spec treats as `100vw`; set it to the
    /// theme's real content width to stop wide viewports over-fetching. An
    /// authored `sizes` on the image always wins.
    pub sizes: Option<String>,
}

/// Build-time image optimization, per format. A format is enabled by naming it
/// in the `optimize { .. }` block (`png`, `jpeg`); an absent format is left
/// untouched. Each format carries its own tuning.
#[derive(Debug, Clone, Hash, Default)]
pub struct OptimizeConfig {
    /// PNG optimization (oxipng), when enabled.
    pub png: Option<PngConfig>,
    /// JPEG optimization (re-encode), when enabled.
    pub jpeg: Option<JpegConfig>,
}

impl OptimizeConfig {
    /// Whether any format is enabled.
    pub fn any(&self) -> bool {
        self.png.is_some() || self.jpeg.is_some()
    }

    /// The enabled format for a file extension. `None` when unrecognized or that
    /// format's optimization is off.
    pub fn format(&self, ext: &str) -> Option<ImageFormat> {
        let matched = ImageFormat::from_ext(ext)?;
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

impl Named for PngStrip {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("none", Self::None),
        ("safe", Self::Safe),
        ("all", Self::All),
    ];
}

/// JPEG optimization tuning (re-encode).
#[derive(Debug, Clone, Hash)]
pub struct JpegConfig {
    /// Re-encode quality, `1`–`100`.
    pub quality: u8,
}

/// `robots.txt` generation. Enabled by the presence of a `generate { robots }`
/// block.
#[derive(Debug, Clone, Hash, Default)]
pub struct RobotsConfig {
    /// Whether to emit `robots.txt`.
    pub enabled: bool,
    /// Paths disallowed for all crawlers. Empty = allow everything.
    pub disallow: Vec<String>,
}

/// `llms.txt` generation ([llmstxt.org]): a Markdown index of the site's pages
/// for LLM consumption. Enabled by the presence of a `generate { llms }` block.
///
/// [llmstxt.org]: https://llmstxt.org
#[derive(Debug, Clone, Hash, Default)]
pub struct LlmsConfig {
    /// Whether to emit `llms.txt`.
    pub enabled: bool,
    /// Optional one-line summary rendered as the blockquote under the title.
    pub summary: Option<String>,
}

/// Asset pipeline options. All opt-in: a fresh site copies assets verbatim.
///
/// CSS is minified with lightningcss, independently of bundling. JavaScript is
/// only processed (bundled *and* minified, via rolldown) when
/// [`AssetConfig::bundle`] is set: the bundler owns the whole JS step.
#[derive(Debug, Clone, Hash, Default)]
pub struct AssetConfig {
    /// Minify CSS (lightningcss) and, when bundling, JavaScript (rolldown).
    pub minify: bool,
    /// Bundle JavaScript entry points through rolldown (resolves imports and
    /// tree-shakes). Required for any JavaScript processing.
    pub bundle: bool,
    /// Content-hash asset filenames (`style.css` -> `style.<hash>.css`) and
    /// rewrite references, for far-future caching.
    pub fingerprint: bool,
    /// Image handling (lazy loading, extraction, optimization, responsive
    /// variants), for both pipeline assets and typst-embedded rasters.
    pub images: ImagesConfig,
}

/// Cache options.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Cache directory.
    pub dir: PathBuf,
    /// Enable incremental builds.
    pub incremental: bool,
}

/// Hand-written so `incremental` stays *out* of the fingerprint, for the same
/// reason [`Mode`] does: a `--no-cache` run still writes the next manifest, and
/// keying it on "caching was off" makes the following normal build a whole-site
/// miss. Destructured, so a new field fails to compile until it is placed.
impl std::hash::Hash for CacheConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            dir,
            incremental: _,
        } = self;
        dir.hash(state);
    }
}

/// External command hooks. Each command runs through the system shell in the
/// project root: the escape hatch for tools baudelaire does not embed
/// (Tailwind, PostCSS, Pagefind, image optimizers, deploy scripts).
#[derive(Debug, Clone, Hash, Default)]
pub struct HooksConfig {
    /// Commands run before the build (before the asset pipeline), so any files
    /// they generate into `assets/` are picked up and fingerprinted.
    pub before: Vec<String>,
    /// Commands run after the site is written to `dist`.
    pub after: Vec<String>,
}

/// Announce destinations for the built site. Each backend is an optional
/// block under `announce { .. }`; adding a destination is one field here plus one
/// backend in [`crate::announce`]. Secrets are never stored here; a backend
/// reads its credentials from the environment at announce time.
#[derive(Debug, Clone, Hash, Default)]
pub struct AnnounceConfig {
    /// standard.site (AT Protocol) target.
    pub standard: Option<StandardConfig>,
}

/// The standard.site (AT Protocol) target.
#[derive(Debug, Clone, Hash)]
pub struct StandardConfig {
    /// Account handle or DID to authenticate as, e.g. `you.bsky.social`.
    pub handle: String,
    /// Repository DID (a stable public identifier, not a secret). When set, the
    /// build emits the standard.site verification artifacts (the `.well-known`
    /// file and per-page `<link>` tags) offline; the announce run checks it against
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

/// Deploy destinations for the built files. Each backend is an optional block
/// under `deploy { .. }`; adding one is a field here plus a backend in
/// [`crate::deploy`]. Credentials are never stored here; a backend reads them
/// from the environment at deploy time.
#[derive(Debug, Clone, Hash, Default)]
pub struct DeployConfig {
    /// An S3-compatible bucket (AWS S3, Cloudflare R2, ..).
    pub s3: Option<S3Config>,
    /// A host reachable over SSH, files transferred with SFTP.
    pub ssh: Option<SshConfig>,
}

/// An S3-compatible bucket target. Works against AWS S3 by default; set
/// `endpoint` for R2 or any S3-compatible host.
#[derive(Debug, Clone, Hash)]
pub struct S3Config {
    /// Bucket name.
    pub bucket: String,
    /// S3 endpoint host, e.g. `https://ACCOUNT.r2.cloudflarestorage.com`. `None`
    /// targets AWS at the region's default host.
    pub endpoint: Option<String>,
    /// Region code. R2 uses `auto`; AWS uses e.g. `us-east-1` (the default).
    pub region: String,
    /// Key prefix every uploaded object is placed under (a subdirectory in the
    /// bucket). Empty by default.
    pub prefix: String,
    /// Delete remote objects under `prefix` that the build no longer produces.
    pub delete: bool,
}

/// A host reachable over SSH. Files are reconciled with the remote directory
/// over SFTP; change detection runs `sha256sum` on the host so an unchanged file
/// is never re-sent. Works against any OpenSSH-compatible server.
#[derive(Debug, Clone, Hash)]
pub struct SshConfig {
    /// Hostname or IP of the server.
    pub host: String,
    /// Absolute path to the remote directory the build is mirrored into.
    pub path: String,
    /// Port the SSH server listens on.
    pub port: u16,
    /// User to authenticate as. Defaults to `$USER`.
    pub user: Option<String>,
    /// Path to a private key (absolute, `~`-relative, or under the project
    /// root). When unset, authentication tries the ssh-agent, then a password
    /// from the environment/prompt.
    pub key: Option<PathBuf>,
    /// Verify the server's host key against `~/.ssh/known_hosts`, learning an
    /// unseen host on first connect and refusing a changed key (MITM guard).
    /// Turn off to accept any key (`StrictHostKeyChecking=no`).
    pub strict: bool,
    /// Delete remote files under `path` that the build no longer produces.
    pub delete: bool,
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
