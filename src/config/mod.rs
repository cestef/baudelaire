//! Baudelaire site configuration.
//!
//! Parsed from `config.kdl`. See [`Config::parse`] and [`Config::default`].
//!
//! One module per config block, nested as the blocks are: a section's struct,
//! its conventional defaults and its `Section` key table sit together. Every
//! type is re-exported here, so the rest of the crate names them flatly.

pub mod announce;
pub mod assets;
pub mod cache;
pub mod caching;
pub mod content;
pub mod deploy;
pub(crate) mod dispatch;
pub mod edit;
pub mod explain;
pub mod generate;
pub mod hooks;
pub mod html;
pub mod key;
pub mod lang;
pub mod links;
pub mod lint;
pub mod named;
pub mod navigation;
mod node;
pub mod paths;
pub mod permalink;
pub mod profile;
pub mod prune;
pub mod redirect;
pub mod reference;
pub mod schema;
pub mod security;
pub mod serve;
#[cfg(test)]
mod tests;
pub mod typst;
mod url;
mod value;
pub mod values;

use std::path::{Path, PathBuf};

use kdl::{KdlDocument, KdlNode};

use crate::config::dispatch::Kind::{Block as Nested, Items, Lines, Overlay, Table, Text, Url};
use crate::config::dispatch::{Attributed, Block, Section};
use crate::config::lang::Rtl;
use crate::config::node::NodeExt;
use crate::content::listing::Titlecase;
use crate::error::{ConfigError, Result, ThemeError};

pub use announce::AnnounceConfig;
pub use announce::standard::{StandardConfig, VerifyConfig};
pub use assets::AssetConfig;
pub use assets::images::ImagesConfig;
pub use assets::images::optimize::{JpegConfig, OptimizeConfig, PngConfig, PngStrip};
pub use assets::images::responsive::ResponsiveConfig;
pub use assets::minify::MinifyConfig;
pub use assets::sourcemap::{SourceMapConfig, SourceMaps};
pub use assets::tailwind::TailwindConfig;
pub use assets::targets::{TargetConfig, Version};
pub use cache::CacheConfig;
pub use caching::CacheControl;
pub use content::ContentConfig;
pub use content::collection::{CollectionConfig, PaginateConfig, SortKey};
pub use content::drafts::DraftConfig;
pub use content::entities::source::{
    DataSource, Declared, InlineSource, PagesSource, SourceConfig, SourcesConfig,
};
pub use content::entities::{RegistryConfig, Shape, Slots, Unknown};
pub use content::markdown::{Extension, MarkdownConfig, RawHtml};
pub use content::reading::ReadingConfig;
pub use content::taxonomy::TaxonomyConfig;
pub use deploy::DeployConfig;
pub use deploy::s3::S3Config;
pub use deploy::ssh::SshConfig;
pub use generate::GenerateConfig;
pub use generate::bundle::{BundleConfig, BundleFormat};
pub use generate::cards::CardsConfig;
pub use generate::feed::{Content, FeedConfig, FeedKind, FeedNames};
pub use generate::headers::HeadersConfig;
pub use generate::llms::LlmsConfig;
pub use generate::manifest::{DisplayMode, IconConfig, IconPurpose, ManifestConfig};
pub use generate::pdf::{PdfConfig, PdfPages};
pub use generate::robots::RobotsConfig;
pub use generate::search::{SearchConfig, SearchField, SearchFormat};
pub use hooks::HooksConfig;
pub use html::anchors::{AnchorConfig, Place};
pub use html::highlight::{HighlightConfig, Token};
pub use html::math::{MathConfig, MathStyles};
pub use html::meta::MetaConfig;
pub use html::region::RegionConfig;
pub use html::{Footnotes, HtmlConfig};
pub use lang::LanguageConfig;
pub use links::external::ExternalConfig;
pub use links::{LinkConfig, Linked};
pub use lint::LintConfig;
pub use lint::budget::BudgetConfig;
pub use lint::headings::HeadingConfig;
pub use lint::rule::{Rule, Ruled};
pub use lint::severity::{Level, Severity};
pub use lint::snippets::SnippetConfig;
pub use named::Named;
pub use navigation::NavigationConfig;
pub use navigation::spa::{Prefetch, SpaConfig};
pub use navigation::speculation::{Eagerness, SpeculationConfig};
pub use navigation::standalone::{Router, StandaloneConfig};
pub use paths::{Paths, Rooted};
pub use permalink::{Permalink, PermalinkCtx, PermalinkError};
pub use prune::PruneConfig;
pub use redirect::RedirectConfig;
pub use schema::{FieldSchema, FieldType, TypeError, Words};
pub use security::SecurityConfig;
pub use security::csp::CspConfig;
pub use serve::ServeConfig;
pub use typst::TypstConfig;
pub use typst::fonts::FontConfig;
pub use url::{BaseUrl, Basename, Percent, Slashed, UrlStyle};
pub use values::Value;

#[derive(Debug, Clone)]
pub struct Config {
    pub site: Option<String>,
    /// Canonical base URL, e.g. `https://example.net`.
    pub url: Option<String>,
    /// Default language code.
    pub lang: String,
    pub author: Option<String>,
    /// What the site is, in one line, for the feed channel.
    ///
    /// Deliberately not a fallback for a page's `<meta name="description">`:
    /// the same sentence on every page reads as duplicate metadata.
    pub description: Option<String>,
    /// The project root: what every other path is relative to, and what typst
    /// resolves `/`-absolute imports against.
    pub root: PathBuf,
    pub paths: Paths,
    /// A theme package supplying templates, assets, and config defaults, named
    /// like any Typst dependency (`@preview/plume:1.0.0`), each of which the
    /// project may override.
    pub theme: Option<String>,
    /// What the content tree contains and how it is read: bundles, drafts,
    /// future dating, collections, taxonomies.
    pub content: ContentConfig,
    /// Declared languages keyed by code. Empty is a single-language site (only
    /// `lang`); a non-empty block turns on i18n, and the default `lang` is a
    /// known language whether or not it appears here.
    pub languages: Vec<(String, LanguageConfig)>,
    pub assets: AssetConfig,
    pub html: HtmlConfig,
    pub links: LinkConfig,
    /// Old paths with no page behind them, each paired with where it moved.
    ///
    /// A frontmatter `redirect` covers a page that still exists; this covers a
    /// URL with nothing left to declare it (a deleted page, a generated index).
    pub redirect: Vec<(String, RedirectConfig)>,
    /// Post-render linting of the built pages: accessibility and structure
    /// rules over the typed DOM, and per-page weight budgets.
    pub lint: LintConfig,
    /// Integrity attributes and the content security policy, both derived from
    /// what the pages actually load and inline.
    pub security: SecurityConfig,
    /// Files the build generates beside the pages: sitemap, robots, llms,
    /// feeds, search indexes, social cards.
    pub generate: GenerateConfig,
    /// How a visitor moves between the built pages: SPA runtime, single-file
    /// export, browser speculation hints.
    pub navigation: NavigationConfig,
    /// Remove orphaned outputs from `dist` on each build: files a previous
    /// build wrote that this one no longer produces. The asset tree and build
    /// cache are never touched.
    pub prune: PruneConfig,
    /// Typst engine knobs (`sys.inputs`, experimental features).
    pub typst: TypstConfig,
    /// Build-time constants exposed to client JS through the `baudelaire:config`
    /// virtual module: arbitrary scalars keyed by name.
    pub client: Vec<(String, crate::codegen::Value)>,
    /// Build cache options: where the incremental manifest lives, and whether
    /// it is consulted. Not `caching`, which is what a *browser* is told.
    pub cache: CacheConfig,
    /// The `Cache-Control` the built files are served with, applied by every
    /// destination that can say so.
    pub caching: CacheControl,
    pub hooks: HooksConfig,
    pub announce: AnnounceConfig,
    pub deploy: DeployConfig,
    pub serve: ServeConfig,
    /// The active profile name, if one was applied (exposed to pages).
    pub profile: Option<String>,
    /// Named profile partials, applied over the base in
    /// [`Config::with_profile`].
    pub profiles: Vec<(String, KdlDocument)>,
    /// The raw `config.kdl` text this config was parsed from: the retained
    /// profile nodes carry spans into this exact string.
    pub(crate) source: String,
}

impl Config {
    /// Parse a site's config, layered over whatever defaults its theme supplies.
    ///
    /// Two passes, because the config is what names the theme: the site's own
    /// text is read once to learn that, then re-applied over the theme's
    /// `theme.kdl`. A site with no theme parses exactly once.
    ///
    /// `root` is the project directory a directory-theme is resolved against,
    /// passed rather than taken from the process cwd.
    ///
    /// `theme` is the `--theme` override, a parameter because it has to land
    /// between learning which theme to fetch and reading its defaults, and to be
    /// re-applied after the second pass, which re-reads the site's own text.
    pub fn load(text: &str, root: &std::path::Path, theme: Option<&str>) -> Result<Self> {
        let requested = |config: Self| match theme {
            Some(theme) => Self {
                theme: Some(theme.to_owned()),
                ..config
            },
            None => config,
        };
        let config = requested(Self {
            root: root.to_path_buf(),
            ..Self::parse(text)?
        });
        let Some(floor) = config.beneath()? else {
            return Ok(config);
        };
        Self::parse_over(floor, text).map(requested)
    }

    /// What this config holds, key by key: every dispatch table read back, so
    /// a value can be reported under the key that parses it.
    pub fn values(&self) -> Value {
        <Self as Section>::values(self)
    }

    /// The theme's own config, read as the floor this one stands on, or `None`
    /// where no theme is named or the one named ships no `theme.kdl`.
    pub fn beneath(&self) -> Result<Option<Self>> {
        let Some(resolved) = crate::theme::Theme::of(self)? else {
            return Ok(None);
        };
        let Some(defaults) = resolved.config() else {
            return Ok(None);
        };
        Self::floor(&defaults, self.root.clone()).map(Some)
    }

    /// A theme's `theme.kdl`, read as the floor the site's own config stands on.
    ///
    /// `root` is the project's and is passed in rather than taken from the
    /// parse. Everything in [`OWNED`](Config::OWNED) is refused outright, since
    /// a theme is fetched at build time and those sections decide what the
    /// machine runs, what the browser trusts in the site's name, or what a
    /// build fails on. Refused rather than dropped, so a theme author finds out.
    fn floor(at: &Path, root: PathBuf) -> Result<Self> {
        let text = crate::fs::read_to_string(at)?;
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(&text, e))?;
        if let Some(section) = doc
            .nodes()
            .iter()
            .map(|node| node.name().value())
            .find(|name| Self::OWNED.contains(name))
        {
            return Err(ThemeError::governs(at.display(), section).into());
        }
        let mut config = Self {
            root,
            source: text.clone(),
            ..Self::default()
        };
        config.apply(doc.nodes(), &text)?;
        if let Some(section) = config.usurped() {
            return Err(ThemeError::governs(at.display(), section).into());
        }
        config.check()?;
        Ok(config)
    }

    /// What a theme's defaults may not carry from *inside* a section it is
    /// otherwise allowed, read off the parsed values because these are keys
    /// rather than whole sections ([`OWNED`](Config::OWNED) refuses those).
    ///
    /// Both let a fetched theme speak to the browser in the site's name: a
    /// `generate { headers { } }` rule is an arbitrary response header on an
    /// arbitrary path, and a *wildcard* `redirect` claims no output file, so
    /// nothing can catch it burying a real page.
    fn usurped(&self) -> Option<&'static str> {
        if !self.generate.headers.rules.is_empty() {
            return Some("generate { headers { .. } }");
        }
        self.redirect
            .iter()
            .any(|(old, _)| Self::wildcard(old))
            .then_some("a wildcard `redirect`")
    }

    /// Apply a config text over an existing config, rather than over the
    /// built-in defaults: how a theme's `theme.kdl` becomes the floor the site's
    /// own config stands on.
    fn parse_over(base: Self, text: &str) -> Result<Self> {
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        let mut config = Self {
            source: text.to_owned(),
            ..base
        };
        config.apply(doc.nodes(), text)?;
        config.check()?;
        Ok(config)
    }

    /// Root of all machine-local, regenerable build state, one subdirectory per
    /// subsystem. [`Scratch`] names them and says what each holds.
    ///
    /// Everything here is derivable, never authored: gitignored, wiped by
    /// `clean`, and safe to delete at any time.
    pub const SCRATCH: &'static str = ".baudelaire";

    /// Whether `path` carries `ext`, compared without case.
    ///
    /// Case-insensitive because the asset pipeline already is, and the two
    /// halves of one build must not disagree about what a `README.MD` is.
    pub fn has_ext(path: &Path, ext: &str) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    /// The extension of a typst page, and of a markdown one.
    pub const TYPST: &'static str = "typ";
    pub const MARKDOWN: &'static str = "md";

    /// Every extension a page can be written with, whatever this binary was
    /// built with and whatever the site asked for: what a *name* is judged
    /// against, where [`Config::sources`] is what a *build* reads.
    pub const SOURCES: &'static [&'static str] = &[Self::TYPST, Self::MARKDOWN];

    /// The extensions of a Sass source, SCSS and the indented syntax.
    ///
    /// Here rather than beside the compiler, because a build without that
    /// compiler still has to recognize one as an input and not a file to
    /// publish.
    pub const SASS: &'static [&'static str] = &["scss", "sass"];

    /// The not-found page's output file: flat at the dist root, the name static
    /// hosts serve for unmatched URLs and what the dev server falls back to.
    pub const NOT_FOUND: &'static str = "404.html";

    /// The config file, by the one name it is spelled: the `--config` default,
    /// what `init` writes, and what a diagnostic names until the loader supplies
    /// the path actually read.
    pub const FILE: &'static str = "config.kdl";

    /// The file a URL ending in `/` is served from, and so the one a clean URL
    /// is written to: `index` plus [`UrlStyle::PAGE`].
    pub const INDEX: &'static str = "index.html";

    /// The URL segment that climbs a directory, refused wherever a page, a
    /// redirect or a permalink template names its own output.
    pub(crate) const PARENT: &'static str = "..";

    /// The key holding the profile partials, shared by the top-level rule that
    /// parses it and the guard refusing one *inside* a profile.
    pub(crate) const PROFILES: &'static str = "profiles";

    /// The other keys named twice, by their row in `RULES` and by `OWNED`.
    const PATHS: &'static str = "paths";
    const HOOKS: &'static str = "hooks";
    const ANNOUNCE: &'static str = "announce";
    const DEPLOY: &'static str = "deploy";
    const SERVE: &'static str = "serve";
    /// Suffixed because [`Config::TYPST`] is already the *extension* `typ`.
    const TYPST_SECTION: &'static str = "typst";
    const SECURITY: &'static str = "security";
    const LINT: &'static str = "lint";
    const CLIENT: &'static str = "client";

    /// The sections a site owns outright, and so the ones a theme's `theme.kdl`
    /// may not carry; `Config::floor` says why.
    pub const OWNED: [&'static str; 10] = [
        Self::PATHS,
        Self::HOOKS,
        Self::ANNOUNCE,
        Self::DEPLOY,
        Self::PROFILES,
        Self::SERVE,
        Self::TYPST_SECTION,
        Self::SECURITY,
        Self::LINT,
        Self::CLIENT,
    ];

    /// The text this was parsed from, which is what `config explain` reads a
    /// key's own line back out of.
    pub fn text(&self) -> &str {
        &self.source
    }

    /// The path of a named scratch subdirectory under
    /// [`SCRATCH`](Config::SCRATCH).
    pub fn scratch(sub: Scratch) -> PathBuf {
        PathBuf::from(Self::SCRATCH).join(sub.dir())
    }

    /// The site's name, or `"unnamed"` when it declares none.
    pub fn label(&self) -> &str {
        self.site.as_deref().unwrap_or("unnamed")
    }

    /// The site title in a given language: the language's `site` override if it
    /// has one, else the site-wide title.
    pub fn title(&self, code: &str) -> &str {
        self.language(code)
            .and_then(|lang| lang.site.as_deref())
            .unwrap_or_else(|| self.label())
    }

    /// What the site is, in a given language: the language's `description`
    /// override if it has one, else the site-wide one. `None` when neither is
    /// set, which is what makes a feed fall back to its title.
    pub fn description(&self, code: &str) -> Option<&str> {
        self.language(code)
            .and_then(|lang| lang.description.as_deref())
            .or(self.description.as_deref())
    }

    /// The file stem that makes a page a *bundle*: it takes its slug from its
    /// parent directory, and the files beside it belong to it. The fallback for
    /// an unset `content { index }`.
    pub fn index(&self) -> &str {
        self.content.index.as_deref().unwrap_or("index")
    }

    /// The extensions a content file may carry, for this site. One entry per
    /// source dialect, so adding one is a line here and an arm in
    /// [`DiscoveryCache::load_page`](crate::content::DiscoveryCache).
    ///
    /// Discovery and [`LinkMap::classify`](crate::render::LinkMap::classify)
    /// both read it and have to agree, or a link to a built page is left as
    /// authored. Markdown needs both the binary built with it and the site
    /// wanting it.
    pub fn sources(&self) -> Vec<&'static str> {
        #[cfg(feature = "markdown")]
        let markdown = self.content.markdown.enabled;
        #[cfg(not(feature = "markdown"))]
        let markdown = false;
        std::iter::once(Self::TYPST)
            .chain(markdown.then_some(Self::MARKDOWN))
            .collect()
    }

    /// The author in a given language: the language's `author` override if it
    /// has one, else the site-wide author.
    pub fn author(&self, code: &str) -> Option<&str> {
        self.language(code)
            .and_then(|lang| lang.author.as_deref())
            .or(self.author.as_deref())
    }

    /// How fast prose reads in a given language: the language's own `wpm` if it
    /// declares one, else the site's `content { reading { wpm } }`.
    pub fn wpm(&self, code: &str) -> usize {
        self.language(code)
            .and_then(|lang| lang.wpm)
            .unwrap_or(self.content.reading.wpm)
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

    /// Whether this build stamps `integrity` attributes: asked for, *and*
    /// backed by content-addressed names.
    ///
    /// Without `fingerprint` an asset URL names whatever is at that path today,
    /// so a page cached from yesterday would pin a digest the file no longer has
    /// and block it.
    pub fn sri(&self) -> bool {
        self.security.sri && self.assets.fingerprint
    }

    /// Whether this build takes the digest of every inline script, style and
    /// `style` attribute for the generated policy.
    ///
    /// Conditional on the policy having somewhere to go: the `_headers` writer
    /// is the only reader of those digests.
    pub fn hashes(&self) -> bool {
        self.generate.headers.enabled && self.security.csp.enabled && self.security.csp.hashes
    }

    /// Whether the HTML is pretty-printed: `html { pretty }`, unless this build
    /// is hashing what it inlines.
    ///
    /// The two cannot both be had: the pretty printer re-indents a script body
    /// *after* the DOM the digest was taken from, so the browser would refuse to
    /// run the page's own script.
    pub fn pretty(&self) -> bool {
        self.html.pretty && !self.hashes()
    }

    /// The path the site is served under, from the `url`'s path component
    /// (`url "https://host/docs"` -> `/docs`); empty for a root-hosted site.
    /// Every on-page root-absolute URL is prefixed with it, leaving the on-disk
    /// layout unchanged.
    pub fn base_path(&self) -> &str {
        self.url.as_deref().map_or("", BaseUrl::path)
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
    #[cfg(feature = "announce")]
    pub(crate) fn verify_did(&self, artifact: impl Fn(&VerifyConfig) -> bool) -> Option<&str> {
        let standard = self.announce.standard.as_ref()?;
        artifact(&standard.verify)
            .then_some(standard.did.as_deref())
            .flatten()
    }

    pub fn collection(&self, id: &str) -> Option<&CollectionConfig> {
        self.content
            .collections
            .iter()
            .find(|(n, _)| n == id)
            .map(|(_, c)| c)
    }

    /// Collection `id`'s own feed in `lang`, or `None` when it has none: the
    /// collection did not ask, it publishes no index for the feed to sit
    /// beside, or a *site* feed already occupies that scope.
    ///
    /// The one answer the file writer and the advertising `<head>` tag share, so
    /// no page points at a feed no build wrote.
    pub fn channel(&self, id: &str, lang: &str) -> Option<Channel> {
        let collection = self.collection(id)?;
        (collection.feed && collection.paginate.enabled)
            .then(|| self.localize(lang, &collection.home(id)))
            .map(|url| url.trim_matches('/').to_owned())
            .filter(|scope| {
                !self
                    .langs()
                    .iter()
                    .any(|other| self.scope(other, "") == *scope)
            })
            .map(|scope| Channel {
                scope,
                title: format!("{} - {}", self.title(lang), Titlecase(id)),
            })
    }

    /// The layout a page renders through: what its own frontmatter names, else
    /// its collection's `template`. `None` means no layout at all, and the
    /// page's own markup is the document.
    ///
    /// Root pages resolve through this like any other, under the [`ROOT`]
    /// collection they are discovered into.
    ///
    /// [`ROOT`]: crate::content::ROOT
    pub fn template_for(&self, collection: &str, own: Option<String>) -> Option<String> {
        own.or_else(|| self.collection(collection).and_then(|c| c.template.clone()))
    }

    /// The frontmatter schema a collection's pages must satisfy, empty when it
    /// declares none.
    pub fn schema(&self, collection: &str) -> &[(String, FieldSchema)] {
        self.collection(collection)
            .map_or(&[], |c| c.schema.as_slice())
    }

    /// The served name of the assets directory: its final path segment, and the
    /// leading segment of every asset URL.
    pub fn asset_name(&self) -> &str {
        self.paths
            .assets
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("assets")
    }

    /// The URL prefix every processed asset is served under.
    pub fn asset_prefix(&self) -> String {
        format!("/{}", self.asset_name())
    }

    /// The URL a processed asset is served at, given its path relative to the
    /// asset root.
    pub fn asset_url(&self, rel: &Path) -> String {
        format!("{}/{}", self.asset_prefix(), Slashed(rel))
    }

    /// The digest that names a published file by its content, or `None` when
    /// `assets { fingerprint }` is off: the one rule both the image
    /// externalizer and a page's own `assets` dict name a colocated file by, so
    /// the URL a template writes is the file the build wrote.
    pub fn digest_of(&self, source: &Path) -> Option<String> {
        self.assets
            .fingerprint
            .then(|| crate::fs::read(source).ok())
            .flatten()
            .map(|bytes| crate::graph::AssetName::digest(&bytes))
    }

    /// The processed assets directory under `dist`: the *published* location.
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

    /// A URL's path segments, joined back with the empty ones dropped.
    ///
    /// `..` segments are dropped here, which is the defense for every URL source
    /// a permalink template's own check does not cover (a frontmatter slug), so
    /// no page can be written outside `dist`.
    fn segments(url: &str) -> String {
        url.split('/')
            .filter(|segment| !segment.is_empty() && *segment != Self::PARENT)
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Whether `url` names a segment that climbs out of the directory it is
    /// resolved against, which [`Config::segments`] drops and every URL a page
    /// or a redirect can name is refused for.
    pub(crate) fn traverses(url: &str) -> bool {
        url.split('/').any(|segment| segment == Self::PARENT)
    }

    /// The file `url` is written to when it names the not-found page, and
    /// `None` for every other URL.
    ///
    /// 404 must be a flat `404.html`; under clean URLs a `404/` dir isn't served
    /// as not-found, and a translated `404.fr.typ` belongs at `{lang}/404.html`
    /// for the same reason. Only a language scope counts: `/notes/404/` is an
    /// ordinary page.
    pub fn not_found(&self, url: &str) -> Option<PathBuf> {
        let trimmed = Self::segments(url);
        let stem = trimmed.strip_suffix(UrlStyle::PAGE).unwrap_or(&trimmed);
        let not_found = Self::NOT_FOUND
            .strip_suffix(UrlStyle::PAGE)
            .unwrap_or(Self::NOT_FOUND);
        if stem == not_found {
            return Some(self.paths.dist.join(Self::NOT_FOUND));
        }
        stem.strip_suffix(not_found)
            .and_then(|head| head.strip_suffix('/'))
            .filter(|scope| self.languages.iter().any(|(code, _)| code == scope))
            .map(|scope| self.paths.dist.join(scope).join(Self::NOT_FOUND))
    }

    /// The file a root-relative URL names under `dist`, for a URL that already
    /// names a file: a card, a page's PDF, a bundled document.
    ///
    /// [`Config::destination`] is its counterpart for a *page* URL, which has no
    /// extension and so has to be given one according to `links { style }`.
    pub fn file(&self, url: &str) -> PathBuf {
        self.paths.dist.join(Self::segments(url))
    }

    /// The file a page URL is written to under `dist`, honoring clean URLs:
    /// the URL-to-file mapping page output and redirect stubs share.
    pub fn destination(&self, url: &str) -> PathBuf {
        if url == "/" {
            return self.paths.dist.join(Self::INDEX);
        }
        if Self::names_a_file(url) {
            return self.file(url);
        }
        if let Some(path) = self.not_found(url) {
            return path;
        }
        let trimmed = Self::segments(url);
        match self.links.style {
            UrlStyle::Clean => self.paths.dist.join(&trimmed).join(Self::INDEX),
            UrlStyle::Flat => self
                .paths
                .dist
                .join(self.links.style.url(&trimmed).trim_start_matches('/')),
        }
    }

    /// Whether `url`'s last segment carries an extension, i.e. names a file
    /// rather than a directory-style page URL.
    ///
    /// Only a frontmatter `path` can produce one under clean URLs: every
    /// generated permalink is directory-shaped, and a flat one ends in `.html`,
    /// which this reads the same way.
    pub(crate) fn names_a_file(url: &str) -> bool {
        url.rsplit('/')
            .next()
            .is_some_and(|last| last.contains('.') && !last.starts_with('.'))
    }

    /// Whether a `redirect` old path is a pattern rather than a path: it
    /// carries a `*`, so it matches a family of URLs and names no file.
    ///
    /// A wildcard can only ever be a rule, since an HTML stub is a file at one
    /// path. What the *target* may say in return (`/:splat`) is the host's
    /// grammar and passes through untouched.
    pub(crate) fn wildcard(old: &str) -> bool {
        old.contains('*')
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

    /// Whether anything in this build binds a page's *prose* rather than its
    /// finished markup: an EPUB chapter, which is the region with the chrome
    /// gone and every URL absolute.
    ///
    /// The capture is a second pass over the DOM, so a site that asked for none
    /// must not pay for it.
    pub fn binds_prose(&self) -> bool {
        self.generate
            .bundles
            .iter()
            .any(|(_, bundle)| bundle.active().contains(&BundleFormat::Epub))
    }

    /// Every language the site builds, default first then declared ones in
    /// config order (default deduplicated).
    pub fn langs(&self) -> Vec<&str> {
        let declared = self.languages.iter().map(|(id, _)| id.as_str());
        std::iter::once(self.lang.as_str())
            .chain(declared.filter(|id| *id != self.lang))
            .collect()
    }

    /// A root-relative `path` under `code`: prefixed with `/{code}` for a
    /// non-default language, unchanged for the default.
    pub fn localize(&self, code: &str, path: &str) -> String {
        match code == self.lang {
            true => path.to_owned(),
            false if path == "/" => format!("/{code}/"),
            false => format!("/{code}{path}"),
        }
    }

    /// The language path segment for `code`: empty for the default, the code
    /// otherwise. It prefixes a generated page's identity and names a language's
    /// output subdirectory, so both mirror the localized URL. `id` is an
    /// optional trailing segment.
    pub fn scope(&self, code: &str, id: &str) -> String {
        self.localize(code, &format!("/{id}"))
            .trim_matches('/')
            .to_owned()
    }
}

/// Feeds every build-affecting setting into the hasher so a config change
/// invalidates the build cache. Destructuring means a newly added field fails to
/// compile until it is accounted for here.
impl std::hash::Hash for Config {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            // Where the project sits, not what it builds: hashing it would undo
            // the portable manifest keys (`mv site site2` must still hit).
            root: _,
            site,
            url,
            lang,
            author,
            description,
            paths,
            theme,
            content,
            languages,
            assets,
            html,
            links,
            redirect,
            // Shapes no markup, but decides what a page *records* while it
            // renders: with linting off a page stores no findings and no weight.
            lint,
            security,
            generate,
            navigation,
            prune,
            typst,
            client,
            cache,
            // `Cache-Control` shapes no page, and the one file it does shape is
            // written by a processor, which runs whatever the cache says.
            caching: _,
            hooks,
            announce,
            deploy,
            // Dev-server settings never affect output, so `serve` on a custom
            // port must not invalidate a `build`'s cache.
            serve: _,
            profile,
            // Applying a profile mutates the fields above, so any change to the
            // raw partials is already captured.
            profiles: _,
            // Kept only for error spans; a comment-only edit must not bust the
            // cache.
            source: _,
        } = self;
        (
            site,
            url,
            lang,
            author,
            description,
            paths,
            theme,
            content,
            languages,
        )
            .hash(state);
        (
            assets, html, links, redirect, lint, security, generate, navigation, prune,
        )
            .hash(state);
        (typst, client, cache, hooks, announce, deploy, profile).hash(state);
    }
}

/// A collection's own syndication feed: where its file goes and what it calls
/// itself, from [`Config::channel`].
pub struct Channel {
    /// The directory the feed files are written to, under the output root and
    /// under the site's own path (`posts`, `fr/posts`).
    pub scope: String,
    /// The feed's title, in the language it is written for.
    pub title: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            site: None,
            url: None,
            lang: "en".into(),
            author: None,
            description: None,
            // The process cwd, which `Root::enter` has already moved to the
            // project directory.
            root: PathBuf::from("."),
            paths: Paths::default(),
            theme: None,
            content: ContentConfig::default(),
            languages: Vec::default(),
            assets: AssetConfig::default(),
            html: HtmlConfig::default(),
            links: LinkConfig::default(),
            redirect: Vec::default(),
            lint: LintConfig::default(),
            security: SecurityConfig::default(),
            generate: GenerateConfig::default(),
            navigation: NavigationConfig::default(),
            prune: PruneConfig::default(),
            typst: TypstConfig::default(),
            client: Vec::default(),
            cache: CacheConfig::default(),
            caching: CacheControl::default(),
            hooks: HooksConfig::default(),
            announce: AnnounceConfig::default(),
            deploy: DeployConfig::default(),
            serve: ServeConfig::default(),
            profile: None,
            profiles: Vec::default(),
            source: String::new(),
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        let mut cfg = Self {
            source: text.to_owned(),
            ..Self::default()
        };
        cfg.apply(doc.nodes(), text)?;
        cfg.check()?;
        Ok(cfg)
    }

    /// Apply a single config node over `self`, used to overlay profile nodes
    /// (see [`Config::with_profile`]).
    pub(crate) fn overlay(&mut self, text: &str, node: &KdlNode) -> Result<()> {
        self.apply(std::slice::from_ref(node), text)
    }
}

/// One subdirectory of [`Config::SCRATCH`], and what lives in it, as a type so
/// that a new one cannot be created without appearing here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scratch {
    /// Incremental build cache: loss forces a full rebuild.
    Cache,
    /// Per-backend announce skip-cache: loss forces idempotent re-sends.
    Announce,
    /// Generated typst modules, their declaration file, and the package mount:
    /// loss is rebuilt on the next compile.
    Generated,
    /// What the external-link check has already seen: loss re-requests every
    /// outbound URL.
    Links,
    /// Code fences written out for a `lint { snippets { } }` command to read:
    /// loss is rewritten by the next lint pass.
    Snippets,
}

impl Scratch {
    pub const fn dir(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Announce => "announce",
            Self::Generated => "generated",
            Self::Links => "links",
            Self::Snippets => "snippets",
        }
    }
}

impl Section for Config {
    const RULES: Block<Self> = Block(&[
        (
            "site",
            Text,
            "The site's name, used in titles, feeds and metadata.",
            |c| c.site.clone().into(),
            |c, n, t| {
                c.site = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "description",
            Text,
            "What the site is, in one line, for the feed channel. Not a per-page `<meta>` fallback.",
            |c| c.description.clone().into(),
            |c, n, t| {
                c.description = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "url",
            Url,
            "The absolute base URL. Sitemaps, feeds and social cards cannot be generated without it.",
            |c| c.url.clone().into(),
            |c, n, t| {
                c.url = Some(n.base_url(t, 0)?);
                Ok(())
            },
        ),
        (
            "lang",
            Text,
            "The default language code, e.g. `en`.",
            |c| c.lang.clone().into(),
            |c, n, t| {
                c.lang = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "author",
            Text,
            "The default author, used by any page naming none.",
            |c| c.author.clone().into(),
            |c, n, t| {
                c.author = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "theme",
            Text,
            "A theme directory whose templates and assets this site layers over.",
            |c| c.theme.clone().into(),
            |c, n, t| {
                c.theme = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            Self::PATHS,
            Nested(Paths::rows),
            "Where the content, output and asset trees live.",
            |c| c.paths.values(),
            |c, n, t| c.paths.fill(n, t),
        ),
        (
            "content",
            Nested(ContentConfig::rows),
            "What the content tree holds and how it is read.",
            |c| c.content.values(),
            |c, n, t| c.content.fill(n, t),
        ),
        (
            "languages",
            Items(LanguageConfig::rows),
            "One block per language, each named by its code.",
            |c| Value::each(&c.languages, Section::values),
            |c, n, t| {
                c.languages = n.unique(t, "language", LanguageConfig::item)?;
                Ok(())
            },
        ),
        (
            "assets",
            Nested(AssetConfig::rows),
            "The pipeline applied to the asset tree.",
            |c| c.assets.values(),
            |c, n, t| c.assets.fill(n, t),
        ),
        (
            "html",
            Nested(HtmlConfig::rows),
            "Post-processing of typst's HTML output.",
            |c| c.html.values(),
            |c, n, t| c.html.fill(n, t),
        ),
        (
            "links",
            Nested(LinkConfig::rows),
            "The shape of generated URLs, and how strictly links are checked.",
            |c| c.links.values(),
            |c, n, t| c.links.fill(n, t),
        ),
        (
            "redirect",
            Lines(RedirectConfig::rows),
            "Old paths no page owns, each forwarded to where its content moved.",
            |c| Value::each(&c.redirect, Attributed::values),
            |c, n, t| {
                c.redirect = n.unique(t, "redirect", RedirectConfig::item)?;
                Ok(())
            },
        ),
        (
            Self::LINT,
            Nested(LintConfig::rows),
            "Checks run over the built pages. Its presence turns them on; `#false` turns them off again.",
            |c| c.lint.values(),
            |c, n, t| c.lint.fill(n, t),
        ),
        (
            Self::SECURITY,
            Nested(SecurityConfig::rows),
            "What the built pages tell a browser to trust.",
            |c| c.security.values(),
            |c, n, t| c.security.fill(n, t),
        ),
        (
            "generate",
            Nested(GenerateConfig::rows),
            "The files a build emits beside the pages.",
            |c| c.generate.values(),
            |c, n, t| c.generate.fill(n, t),
        ),
        (
            "navigation",
            Nested(NavigationConfig::rows),
            "How a visitor moves between the built pages.",
            |c| c.navigation.values(),
            |c, n, t| c.navigation.fill(n, t),
        ),
        (
            "prune",
            Nested(PruneConfig::rows),
            "Delete anything under the output directory that this build did not produce. On by default; `#false` turns it off, and `keep` narrows it.",
            |c| c.prune.values(),
            |c, n, t| c.prune.fill(n, t),
        ),
        (
            "cache",
            Nested(CacheConfig::rows),
            "Where incremental build state lives, and whether to use it.",
            |c| c.cache.values(),
            |c, n, t| c.cache.fill(n, t),
        ),
        (
            "caching",
            Nested(CacheControl::rows),
            "The `Cache-Control` policy uploaded files are given. Its presence turns it on; `#false` turns it off again.",
            |c| c.caching.values(),
            |c, n, t| c.caching.fill(n, t),
        ),
        (
            Self::TYPST_SECTION,
            Nested(TypstConfig::rows),
            "Typst engine knobs: language features, inputs, fonts, package registry.",
            |c| c.typst.values(),
            |c, n, t| c.typst.fill(n, t),
        ),
        (
            Self::CLIENT,
            Table,
            "Constants exposed to client-side JavaScript, one `key value` line per entry.",
            |c| Value::each(&c.client, |value| value.into()),
            |c, n, t| {
                c.client = n.table(t)?;
                Ok(())
            },
        ),
        (
            Self::HOOKS,
            Nested(HooksConfig::rows),
            "External commands run before and after the build.",
            |c| c.hooks.values(),
            |c, n, t| c.hooks.fill(n, t),
        ),
        (
            Self::ANNOUNCE,
            Nested(AnnounceConfig::rows),
            "Where to announce the site's metadata.",
            |c| c.announce.values(),
            |c, n, t| c.announce.fill(n, t),
        ),
        (
            Self::DEPLOY,
            Nested(DeployConfig::rows),
            "Where `baudelaire deploy` uploads the built site.",
            |c| c.deploy.values(),
            |c, n, t| c.deploy.fill(n, t),
        ),
        (
            Self::SERVE,
            Nested(ServeConfig::rows),
            "The development server.",
            |c| c.serve.values(),
            |c, n, t| c.serve.fill(n, t),
        ),
        (
            Self::PROFILES,
            Overlay,
            "Named overlays, each selected with `--profile` and each accepting any key on this page.",
            |c| c.profiles.iter().map(|(name, _)| name.clone()).collect(),
            |c, n, t| {
                c.profiles = n.unique(t, "profile", |child, t| {
                    Ok((child.name().value().to_owned(), child.block(t)?.clone()))
                })?;
                Ok(())
            },
        ),
    ]);
}
