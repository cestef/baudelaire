//! Baudelaire site configuration.
//!
//! Parsed from `config.kdl`. See [`Config::parse`] and [`Config::default`].
//!
//! One module per config block, nested as the blocks are: a section's struct,
//! its conventional defaults and its `Section` key table sit together, so
//! adding a key is one edit in one file rather than three in three. Every type
//! is re-exported here, so the rest of the crate names them flatly.

pub mod announce;
pub mod assets;
pub mod cache;
pub mod caching;
pub mod content;
pub mod deploy;
pub(crate) mod dispatch;
pub mod generate;
pub mod hooks;
pub mod html;
pub mod lang;
pub mod links;
pub mod lint;
pub mod named;
pub mod navigation;
mod node;
pub mod paths;
pub mod permalink;
pub mod profile;
pub mod reference;
pub mod schema;
pub mod security;
pub mod serve;
#[cfg(test)]
mod tests;
pub mod typst;
mod url;
mod value;

use std::path::{Path, PathBuf};

use kdl::{KdlDocument, KdlNode};

use crate::config::dispatch::Kind::{Block as Nested, Flag, Items, Overlay, Table, Text, Url};
use crate::config::dispatch::{Block, Section};
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
pub use cache::CacheConfig;
pub use caching::CacheControl;
pub use content::ContentConfig;
pub use content::collection::{CollectionConfig, PaginateConfig, SortKey};
pub use content::drafts::DraftConfig;
pub use content::markdown::{Extension, MarkdownConfig, RawHtml};
pub use content::taxonomy::TaxonomyConfig;
pub use deploy::DeployConfig;
pub use deploy::s3::S3Config;
pub use deploy::ssh::SshConfig;
pub use generate::GenerateConfig;
pub use generate::cards::CardsConfig;
pub use generate::feed::{FeedConfig, FeedKind, FeedNames};
pub use generate::llms::LlmsConfig;
pub use generate::manifest::{DisplayMode, IconConfig, IconPurpose, ManifestConfig};
pub use generate::pdf::{PdfBundle, PdfConfig, PdfPages};
pub use generate::robots::RobotsConfig;
pub use generate::search::{SearchConfig, SearchField, SearchFormat};
pub use hooks::HooksConfig;
pub use html::highlight::HighlightConfig;
pub use html::{Footnotes, HtmlConfig};
pub use lang::LanguageConfig;
pub use links::{LinkConfig, Linked};
pub use lint::LintConfig;
pub use lint::budget::BudgetConfig;
pub use named::Named;
pub use navigation::NavigationConfig;
pub use navigation::spa::{Prefetch, SpaConfig};
pub use navigation::speculation::{Eagerness, SpeculationConfig};
pub use navigation::standalone::{Router, StandaloneConfig};
pub use paths::{Paths, Rooted};
pub use permalink::{Permalink, PermalinkCtx, PermalinkError};
pub use schema::{FieldSchema, FieldType, TypeError, Words};
pub use security::SecurityConfig;
pub use security::csp::CspConfig;
pub use serve::ServeConfig;
pub use typst::TypstConfig;
pub use url::{BaseUrl, Basename, Percent, UrlStyle};

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
    /// What the site is, in one line. A feed channel needs one, and RSS makes
    /// it mandatory, so without this a reader shows the site title twice.
    ///
    /// Deliberately not a fallback for a page's `<meta name="description">`:
    /// the same sentence stamped on every page is what search engines read as
    /// duplicate metadata. A page describes itself, or says nothing.
    pub description: Option<String>,
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
    /// Old paths with no page behind them, each paired with where it moved.
    ///
    /// A frontmatter `redirect` covers a page that still exists and can speak
    /// for itself. This covers everything else that used to be a URL: a
    /// paginated `page/1/` another generator wrote, a renamed term listing, a
    /// section that is gone. Neither a generated index nor a deleted page has
    /// frontmatter to declare anything in, so the claim has to live here.
    pub redirect: Vec<(String, String)>,
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
    /// build wrote that this one no longer produces (a deleted page, a renamed
    /// permalink, a dropped taxonomy term). The asset tree and build cache are
    /// never touched. Set with `prune #true | #false`.
    pub prune: bool,
    /// Typst engine knobs (`sys.inputs`, experimental features).
    pub typst: TypstConfig,
    /// Build-time constants exposed to client JS through the `baudelaire:config`
    /// virtual module: arbitrary scalars keyed by name.
    pub client: Vec<(String, crate::codegen::Value)>,
    /// Build cache options: where the incremental manifest lives, and whether
    /// it is consulted. Not to be confused with `caching`, which is what a
    /// *browser* is told about the built files.
    pub cache: CacheConfig,
    /// The `Cache-Control` the built files are served with, applied by every
    /// destination that can say so.
    pub caching: CacheControl,
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
    /// changed into the project (a test, an embedding) resolves correctly. It is
    /// recorded on the returned config, so the theme the engine later resolves
    /// is the one resolved here.
    pub fn load(text: &str, root: &std::path::Path) -> Result<Self> {
        let config = Self {
            root: root.to_path_buf(),
            ..Self::parse(text)?
        };
        let Some(theme) = crate::theme::Theme::of(&config)? else {
            return Ok(config);
        };
        let Some(defaults) = theme.config() else {
            return Ok(config);
        };
        Self::parse_over(Self::floor(&defaults, config.root)?, text)
    }

    /// A theme's `theme.kdl`, read as the floor the site's own config stands on.
    ///
    /// Two things it does not get a say in. The first is `root`: the theme's
    /// defaults are a floor for what the site *builds*, and where it is being
    /// built is the project's, so the project's root is passed in rather than
    /// taken from the parse.
    ///
    /// The second is anything that decides what the machine does. A theme is
    /// *fetched*: a package theme is downloaded at build time, so its
    /// `theme.kdl` need never appear in the site's repository, and a section of
    /// it naming a command or a destination would be an instruction the site
    /// never wrote and cannot read. [`HooksConfig`] runs each entry through a
    /// shell, `deploy` and `announce` say where the built site goes and with
    /// which credentials, `paths` decides which trees the build reads and which
    /// it prunes, and `profiles` is raw KDL that can carry any of them. Every
    /// one of those is refused rather than dropped, so a theme author finds out
    /// their block does nothing instead of shipping one that silently never
    /// applies.
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
        config.check()?;
        Ok(config)
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
        config.check()?;
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

    /// The config file, by the one name it is spelled: the `--config` default,
    /// what `init` writes, and what a diagnostic names its source until the
    /// loader supplies the path actually read.
    pub const FILE: &'static str = "config.kdl";

    /// The file a URL ending in `/` is served from, and so the one a clean URL
    /// is written to: `index` plus [`UrlStyle::PAGE`].
    ///
    /// Single source for both ends of that agreement. The build writes it
    /// ([`Config::destination`]) and the dev server looks for it, and a
    /// disagreement is a page that builds and 404s.
    pub const INDEX: &'static str = "index.html";

    /// The key holding the profile partials, shared by the top-level rule that
    /// parses it and the guard refusing one *inside* a profile.
    pub(crate) const PROFILES: &'static str = "profiles";

    /// The four other keys named twice: once by their row in the `RULES` table
    /// below, once by `OWNED`. Spelled once each so the table and the guard
    /// cannot drift apart.
    const PATHS: &'static str = "paths";
    const HOOKS: &'static str = "hooks";
    const ANNOUNCE: &'static str = "announce";
    const DEPLOY: &'static str = "deploy";

    /// The sections a site owns outright, and so the ones a theme's `theme.kdl`
    /// may not carry: see `Config::floor`, which is the only thing that reads
    /// this and the only place the reason is written.
    const OWNED: [&'static str; 5] = [
        Self::PATHS,
        Self::HOOKS,
        Self::ANNOUNCE,
        Self::DEPLOY,
        Self::PROFILES,
    ];

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

    /// What the site is, in a given language: the language's `description`
    /// override if it has one, else the site-wide one. `None` when neither is
    /// set, which is what makes a feed fall back to its title.
    pub fn description(&self, code: &str) -> Option<&str> {
        self.language(code)
            .and_then(|lang| lang.description.as_deref())
            .or(self.description.as_deref())
    }

    /// The file stem that makes a page a *bundle*: it takes its slug from its
    /// parent directory, and the files beside it belong to it.
    ///
    /// THE fallback for an unset `content { index }`. It was spelled at three
    /// call sites (the slug reader, `new --bundle`, the page assembler), each
    /// with its own `"index"` literal, which is three places to change a default
    /// that has to be one.
    pub fn index(&self) -> &str {
        self.content.index.as_deref().unwrap_or("index")
    }

    /// The extensions a content file may carry, for this site. One entry per
    /// source dialect, so adding one is a line here and an arm in
    /// [`DiscoveryCache::load_page`](crate::content::DiscoveryCache), not a new
    /// pipeline.
    ///
    /// Two subsystems ask, and they have to agree: discovery, deciding what is
    /// a page, and [`LinkMap::classify`](crate::render::LinkMap::classify),
    /// deciding whether a link names one. When they disagreed, a `.md` page was
    /// built and published and a link to it was left as authored, so the site
    /// served a dead relative href out of a green build.
    ///
    /// Markdown answers to two things, and both have to say yes: the binary has
    /// to have been built with it, and the site has to want it. A site that says
    /// `content { markdown #false }` keeps its `.md` files as files, and a link
    /// to one stays the file link it was written as.
    pub fn sources(&self) -> Vec<&'static str> {
        // Answered once, per flavor, so neither build carries a branch the
        // other's `cfg` leaves dangling.
        #[cfg(feature = "markdown")]
        let markdown = self.content.markdown.enabled;
        #[cfg(not(feature = "markdown"))]
        let markdown = false;
        std::iter::once("typ")
            .chain(markdown.then_some("md"))
            .collect()
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

    /// Whether this build stamps `integrity` attributes: asked for, *and*
    /// backed by content-addressed names.
    ///
    /// The single gate, shared by the asset pipeline (which pays for the digest
    /// only if one is going to be used), the transform that stamps it, and the
    /// [`Inert`] row that explains the silence. Without `fingerprint` an asset
    /// URL names whatever is at that path today, so a page cached from
    /// yesterday would pin a digest the file no longer has and block it.
    ///
    /// [`Inert`]: crate::engine
    pub fn sri(&self) -> bool {
        self.security.sri && self.assets.fingerprint
    }

    /// Whether this build takes the digest of every inline script, style and
    /// `style` attribute for the generated policy.
    ///
    /// Conditional on the policy having somewhere to go. The digests are read
    /// by exactly one thing, the `_headers` writer, so a site that generates no
    /// `_headers` would pay for them, and pay again in [`Config::pretty`], to
    /// produce a policy nobody is ever served.
    pub fn hashes(&self) -> bool {
        self.generate.headers && self.security.csp.enabled && self.security.csp.hashes
    }

    /// Whether the HTML is pretty-printed: `html { pretty }`, unless this build
    /// is hashing what it inlines.
    ///
    /// The two cannot both be had. A browser digests the bytes between
    /// `<script>` and `</script>` exactly as they are served, and typst's pretty
    /// printer re-indents a script or style body on its way out, *after* the DOM
    /// this build took its digest from. A policy built that way names a body
    /// that was never served, and the browser refuses to run the page's own
    /// script: a site broken in production and nowhere else. Printing the
    /// markup unindented costs nothing but the look of the source.
    pub fn pretty(&self) -> bool {
        self.html.pretty && !self.hashes()
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

    /// Collection `id`'s own feed in `lang`, or `None` when it has none.
    ///
    /// The single answer, because two things have to agree about it: the
    /// processor that writes the file and the `<head>` tag every member
    /// advertises it with. A page pointing at a feed no build wrote is a dead
    /// subscribe button, and nothing downstream would notice. Both facts come
    /// back together for the same reason.
    ///
    /// Three ways to have none: the collection did not ask, it publishes no
    /// index for the feed to sit beside (a feed's `<link>` would name a page
    /// nobody wrote), or it sits exactly where a *site* feed already does, in
    /// which case that file is taken and the site feed, the more inclusive of
    /// the two, keeps it.
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
    /// collection they are discovered into, which is what lets a config (or a
    /// theme's) bind them without every one of them naming a file by hand.
    ///
    /// The one place the order is written, so `new` scaffolds the template the
    /// build will later pick rather than a second opinion about it.
    ///
    /// [`ROOT`]: crate::content::ROOT
    pub fn template_for(&self, collection: &str, own: Option<String>) -> Option<String> {
        own.or_else(|| self.collection(collection).and_then(|c| c.template.clone()))
    }

    /// The frontmatter schema a collection's pages must satisfy, empty when it
    /// declares none (and for a collection with no config block at all).
    pub fn schema(&self, collection: &str) -> &[(String, FieldSchema)] {
        self.collection(collection)
            .map_or(&[], |c| c.schema.as_slice())
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

    /// The URL a processed asset is served at, given its path relative to the
    /// asset root. Separators become `/` whatever the host filesystem writes,
    /// since this is a URL and not a path.
    ///
    /// One derivation, because two layers build these: the pipeline keys its
    /// map with them, and the render pass points an `<img>` at one when the
    /// picture it found is a file the pipeline already owns. A URL built two
    /// ways is a `srcset` that silently stops matching its source.
    pub fn asset_url(&self, rel: &Path) -> String {
        format!(
            "{}/{}",
            self.asset_prefix(),
            rel.to_string_lossy().replace('\\', "/")
        )
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

    /// A URL's path segments, joined back with the empty ones dropped.
    ///
    /// `..` segments are dropped here: permalink *templates* are already
    /// rejected at config parse, and this filter owns the defense for every
    /// other URL source (e.g. a frontmatter slug), so no page can ever be
    /// written outside `dist`.
    fn segments(url: &str) -> String {
        url.split('/')
            .filter(|segment| !segment.is_empty() && *segment != "..")
            .collect::<Vec<_>>()
            .join("/")
    }

    /// The file `url` is written to when it names the not-found page, and
    /// `None` for every other URL.
    ///
    /// 404 must be a flat `404.html`; under clean URLs a `404/` dir isn't
    /// served as not-found. A translated `404.fr.typ` localizes to
    /// `/{lang}/404/` and belongs at `{lang}/404.html` for the same reason.
    /// Only a language scope counts: `/notes/404/` is an ordinary page.
    ///
    /// The single test for "is this the not-found page", shared by
    /// [`destination`](Config::destination) and [`Page::listed`], so the page
    /// held out of navigation is exactly the one written where a host looks for
    /// an unmatched URL.
    ///
    /// [`Page::listed`]: crate::content::Page::listed
    pub fn not_found(&self, url: &str) -> Option<PathBuf> {
        let trimmed = Self::segments(url);
        let stem = trimmed.strip_suffix(UrlStyle::PAGE).unwrap_or(&trimmed);
        // the not-found page's URL stem, derived so its name is written once
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
    /// [`Config::destination`] is its counterpart for a *page* URL, which has
    /// no extension and so has to be given one according to `links { style }`.
    /// Both exist because the two questions have different answers, and every
    /// artifact asking either of them asks it here: three of them derived their
    /// own `dist.join(..)` and nothing tied the answers together.
    pub fn file(&self, url: &str) -> PathBuf {
        self.paths.dist.join(url.trim_start_matches('/'))
    }

    /// The file a page URL is written to under `dist`, honoring clean URLs.
    /// Single source for the URL-to-file mapping, shared by page output and
    /// redirect stubs.
    pub fn destination(&self, url: &str) -> PathBuf {
        if url == "/" {
            return self.paths.dist.join(Self::INDEX);
        }
        // A URL that already names a file is one: a page whose frontmatter
        // `path` spells the old `/2019/post.html` a migration is preserving
        // must not be given a directory and an `index.html` inside it.
        if Self::names_a_file(url) {
            return self.file(url);
        }
        if let Some(path) = self.not_found(url) {
            return path;
        }
        let trimmed = Self::segments(url);
        match self.links.style {
            UrlStyle::Clean => self.paths.dist.join(&trimmed).join(Self::INDEX),
            // A flat page URL already names its file; a raw path (a frontmatter
            // `redirect` old-path) still needs the extension.
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
            // renders: with linting off a page stores no findings and no
            // weight. Leaving it out would let a build with the rules turned on
            // serve those pages from cache and report nothing, which is the one
            // failure mode a gate must not have.
            lint,
            // Shapes the markup (an `integrity` attribute) and the digests a
            // page records, so a page built under one policy must not be served
            // under another.
            security,
            generate,
            navigation,
            prune,
            typst,
            client,
            cache,
            // `Cache-Control` shapes no page. The one file it does shape,
            // `_headers`, is written by a processor, and processors run on
            // every build whatever the cache says; keying pages on it would
            // cold-rebuild the site over a header string.
            caching: _,
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
///
/// One value rather than two lookups, because the file the build writes and the
/// tag a page advertises it with are the same feed, and a reader sees the tag's
/// title before ever fetching the file.
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
            prune: true,
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
        // keep the raw text: profile overlay reports errors against it, its nodes carry spans into it
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

/// The top-level config schema. This table is the *single source of truth* for
/// what keys are valid: dispatch and "unknown key" suggestions both read it.
impl Section for Config {
    const RULES: Block<Self> = Block(&[
        (
            "site",
            Text,
            "The site's name, used in titles, feeds and metadata.",
            |c, n, t| {
                c.site = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "description",
            Text,
            "What the site is, in one line, for the feed channel. Not a per-page `<meta>` fallback.",
            |c, n, t| {
                c.description = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "url",
            Url,
            "The absolute base URL. Sitemaps, feeds and social cards cannot be generated without it.",
            |c, n, t| {
                c.url = Some(n.base_url(t, 0)?);
                Ok(())
            },
        ),
        (
            "lang",
            Text,
            "The default language code, e.g. `en`.",
            |c, n, t| {
                c.lang = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "author",
            Text,
            "The default author, used by any page naming none.",
            |c, n, t| {
                c.author = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "theme",
            Text,
            "A theme directory whose templates and assets this site layers over.",
            |c, n, t| {
                c.theme = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            Self::PATHS,
            Nested(Paths::rows),
            "Where the content, output and asset trees live.",
            |c, n, t| c.paths.fill(n, t),
        ),
        (
            "content",
            Nested(ContentConfig::rows),
            "What the content tree holds and how it is read.",
            |c, n, t| c.content.fill(n, t),
        ),
        (
            "languages",
            Items(LanguageConfig::rows),
            "One block per language, each named by its code.",
            |c, n, t| {
                c.languages = n.unique(t, "language", LanguageConfig::item)?;
                Ok(())
            },
        ),
        (
            "assets",
            Nested(AssetConfig::rows),
            "The pipeline applied to the asset tree.",
            |c, n, t| c.assets.fill(n, t),
        ),
        (
            "html",
            Nested(HtmlConfig::rows),
            "Post-processing of typst's HTML output.",
            |c, n, t| c.html.fill(n, t),
        ),
        (
            "links",
            Nested(LinkConfig::rows),
            "The shape of generated URLs, and how strictly links are checked.",
            |c, n, t| c.links.fill(n, t),
        ),
        (
            "redirect",
            Table,
            "Old paths no page owns, each forwarded to where its content moved.",
            |c, n, t| {
                c.redirect = n.pairs(t)?;
                Ok(())
            },
        ),
        (
            "lint",
            Nested(LintConfig::rows),
            "Checks run over the built pages. Its presence turns them on; `#false` turns them off again.",
            |c, n, t| c.lint.fill(n, t),
        ),
        (
            "security",
            Nested(SecurityConfig::rows),
            "What the built pages tell a browser to trust.",
            |c, n, t| c.security.fill(n, t),
        ),
        (
            "generate",
            Nested(GenerateConfig::rows),
            "The files a build emits beside the pages.",
            |c, n, t| c.generate.fill(n, t),
        ),
        (
            "navigation",
            Nested(NavigationConfig::rows),
            "How a visitor moves between the built pages.",
            |c, n, t| c.navigation.fill(n, t),
        ),
        (
            "prune",
            Flag,
            "Delete anything under the output directory that this build did not produce.",
            |c, n, t| {
                c.prune = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "cache",
            Nested(CacheConfig::rows),
            "Where incremental build state lives, and whether to use it.",
            |c, n, t| c.cache.fill(n, t),
        ),
        (
            "caching",
            Nested(CacheControl::rows),
            "The `Cache-Control` policy uploaded files are given. Its presence turns it on; `#false` turns it off again.",
            |c, n, t| c.caching.fill(n, t),
        ),
        (
            "typst",
            Nested(TypstConfig::rows),
            "Typst engine knobs: language features, inputs, package registry.",
            |c, n, t| c.typst.fill(n, t),
        ),
        (
            "client",
            Table,
            "Constants exposed to client-side JavaScript, one `key value` line per entry.",
            |c, n, t| {
                c.client = n.table(t)?;
                Ok(())
            },
        ),
        (
            Self::HOOKS,
            Nested(HooksConfig::rows),
            "External commands run before and after the build.",
            |c, n, t| c.hooks.fill(n, t),
        ),
        (
            Self::ANNOUNCE,
            Nested(AnnounceConfig::rows),
            "Where to announce the site's metadata.",
            |c, n, t| c.announce.fill(n, t),
        ),
        (
            Self::DEPLOY,
            Nested(DeployConfig::rows),
            "Where `baudelaire deploy` uploads the built site.",
            |c, n, t| c.deploy.fill(n, t),
        ),
        (
            "serve",
            Nested(ServeConfig::rows),
            "The development server.",
            |c, n, t| c.serve.fill(n, t),
        ),
        (
            Self::PROFILES,
            Overlay,
            "Named overlays, each selected with `--profile` and each accepting any key on this page.",
            |c, n, t| {
                c.profiles = n.unique(t, "profile", |child, t| {
                    Ok((child.name().value().to_owned(), child.block(t)?.clone()))
                })?;
                Ok(())
            },
        ),
    ]);
}
