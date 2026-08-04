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
pub mod reference;
pub mod schema;
#[cfg(test)]
mod tests;
mod url;
mod value;

use std::path::{Path, PathBuf};

use kdl::KdlDocument;

use crate::config::dispatch::Section;
use crate::error::{ConfigError, Result};
use crate::mime::ImageFormat;
use crate::ui::Bytes;

pub use permalink::{Permalink, PermalinkCtx, PermalinkError};
pub use schema::{FieldSchema, FieldType, TypeError};
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

    /// The directories typst sees, as *it* spells them: relative to the project
    /// root, which is how a span, a dependency path and an import all name a
    /// file.
    ///
    /// Both sides go through [`crate::fs::resolved`], the spelling the link map
    /// and the dependency tracker already key on, because either can be reached
    /// through a symlink: comparing them lexically leaves a configured directory
    /// looking like it sits outside the very root it is under.
    pub fn under(&self, root: &Path) -> Rooted {
        let root = crate::fs::resolved(root);
        let relative = |dir: &Path| {
            let dir = crate::fs::resolved(dir);
            dir.strip_prefix(&root)
                .map_or_else(|_| dir.clone(), Path::to_path_buf)
        };
        Rooted {
            content: relative(&self.content),
            templates: relative(&self.templates),
        }
    }
}

/// The configured source directories in the compiler's spelling, from
/// [`Paths::under`]. Only the two typst reads: `dist`, `assets` and `static` are
/// walked by the build itself and never named in a span or an import.
///
/// A directory outside the root keeps its absolute path: there is no
/// root-relative spelling of it, and inventing one would name a different place.
pub struct Rooted {
    /// Where pages are authored: what a link's origin is tested against to tell
    /// an author's own reference from a layout's chrome.
    pub content: PathBuf,
    /// Where layouts live: what a wrapper's root-absolute `#import` resolves
    /// against.
    pub templates: PathBuf,
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
    /// A mirror of Typst Universe to download the `preview` namespace from,
    /// without the trailing slash. `None` is the official registry.
    ///
    /// It covers a page's own `#import` and the site's theme alike, since both
    /// resolve through the same store. Only `preview` is affected: every other
    /// namespace is served from the local package directories and never fetched.
    pub registry: Option<String>,
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
        // The project's root, not the theme's default: `theme.kdl` is a floor
        // for what the site *builds*, and where it is being built is not one of
        // the things it gets a say in.
        let base = Self {
            root: config.root,
            ..Self::parse(&crate::fs::read_to_string(&defaults)?)?
        };
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
            return self.paths.dist.join("index.html");
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
            UrlStyle::Clean => self.paths.dist.join(&trimmed).join("index.html"),
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
    /// The generated index over this collection's members.
    pub paginate: PaginateConfig,
    /// What every member's frontmatter must declare, in declaration order.
    /// Empty is the default: nothing required, nothing typed.
    pub schema: Vec<(String, FieldSchema)>,
}

/// A collection's generated index: whether there is one, how it is chunked, and
/// where it is served.
///
/// One block, because it is one concept. It used to be four flat attributes
/// sitting beside the ones that shape *member* pages, and three of the four
/// names did not say what they meant: `list` was a template rather than a list,
/// `mount` the permalink of page 1, `prefix` the segment before a page number.
/// Reading `template` next to `list` gave no clue that the first wrapped a post
/// and the second the index over them.
#[derive(Debug, Clone, Hash)]
pub struct PaginateConfig {
    /// Whether an index is generated at all: the block's presence.
    pub enabled: bool,
    /// Members per page. `None` puts every member on one page, which is what a
    /// listing with no size is.
    pub size: Option<usize>,
    /// Template for the generated index pages, as distinct from the collection's
    /// `template`, which wraps its members.
    pub template: Option<String>,
    /// Where page 1 is served. `None` = `/{id}/`; set to `/` to mount a blog at
    /// the site root.
    pub mount: Option<String>,
    /// Path segment before a page number: `/{id}/{prefix}/{n}/`. Defaults to
    /// `page` (`/blog/page/2/`); empty drops the segment (`/blog/2/`).
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
    /// Per-language description override (else the site-wide `description`), so
    /// a French feed does not carry an English blurb.
    pub description: Option<String>,
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
    /// Members per term page. `None` puts every member on one page, which is
    /// what a term listing used to do unconditionally, beside a collection
    /// index that paginated the same pages.
    pub paginate: Option<usize>,
    /// Path segment before a term page's number (`/tags/rust/page/2/`); empty
    /// drops it. Spelled like a collection's, since it is the same thing.
    pub prefix: String,
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
    /// Hand each page the pages whose content links to it, as `page.backlinks`.
    ///
    /// Opt-in because it is the one page value that cannot be known before the
    /// site has rendered: a page whose backlinks turn out wrong is compiled a
    /// second time (see `engine::links::Graph`), which a site that shows none
    /// should not pay for.
    pub backlinks: bool,
    /// Report the pages nothing links to, and what counts as a link. `None`
    /// leaves the report off.
    pub orphans: Option<Linked>,
}

/// What counts as pointing at a page, for the orphan report.
///
/// A layout never does under either: a sidebar links every page from every page,
/// so counting one would mean no page is ever an orphan. The difference is
/// whether a page the *build* generated counts as a reader's way in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Linked {
    /// Any page's link. A post reached from its paginated index or from a term
    /// page is reached, so the report names only what a reader cannot get to at
    /// all.
    #[default]
    Any,
    /// Only a link on a page an author wrote. A post reached from its index and
    /// from nowhere else is named, which is the question a documentation site
    /// asks: did anyone write about this page?
    Authored,
}

impl Named for Linked {
    const NAMES: &'static [(&'static str, Self)] =
        &[("any", Self::Any), ("authored", Self::Authored)];
}

impl Linked {
    /// Whether a link on this page counts. `generated` is whether the build
    /// wrote the page rather than an author.
    pub fn counts(self, generated: bool) -> bool {
        !generated || self == Self::Any
    }
}

impl LinkConfig {
    /// Whether this build needs the site's link graph at all.
    ///
    /// The one gate the render pass records edges behind, and the one both
    /// readers of them share: a page's backlinks and the orphan report are the
    /// same graph asked two questions. Without it nothing walks a link's origin
    /// and no page carries the edges in its cache entry.
    pub fn graph(&self) -> bool {
        self.backlinks || self.orphans.is_some()
    }
}

/// What the built pages tell a browser to trust: the integrity of the files
/// they load, and the policy they are served under.
///
/// Both are derived from the pages themselves rather than written by hand,
/// which is the only way either stays true: a hand-kept `script-src` goes stale
/// the moment a template gains an inline script, and a hand-kept `integrity`
/// the moment the file it names is rebuilt.
#[derive(Debug, Clone, Default, Hash)]
pub struct SecurityConfig {
    /// Stamp `integrity` onto every script and stylesheet this build emitted,
    /// so a browser refuses one that arrives altered.
    ///
    /// Needs `assets { fingerprint }`: an attribute pinning a digest to a URL
    /// whose contents can change under it is how a site serves a page that
    /// blocks its own stylesheet.
    pub sri: bool,
    /// The `Content-Security-Policy` written into the generated `_headers`.
    pub csp: CspConfig,
}

/// A generated `Content-Security-Policy`.
///
/// Each directive is the value it is given, verbatim: a CSP source list is its
/// own small language (`'self'`, `https:`, a host, `'unsafe-inline'`), and
/// inventing a second spelling for it would help nobody. What this adds is the
/// half no author can write down, the digest of every inline script and style
/// the build produced.
#[derive(Debug, Clone, Hash)]
pub struct CspConfig {
    /// Whether a policy is emitted at all; flipped by the block's presence.
    pub enabled: bool,
    /// Enforce it. Off emits `Content-Security-Policy-Report-Only`, which
    /// reports violations and blocks nothing: how a policy is rolled out.
    pub enforce: bool,
    /// Add the digest of every inline `<script>` and `<style>` the build
    /// produced to the script and style directives, which is what lets a strict
    /// policy coexist with the inline blocks a page needs.
    pub hashes: bool,
    /// `default-src`, the fallback every unstated fetch directive inherits.
    pub default: Option<String>,
    /// `script-src`, `style-src`, and the rest, each stated only if set.
    pub script: Option<String>,
    pub style: Option<String>,
    pub img: Option<String>,
    pub font: Option<String>,
    pub connect: Option<String>,
    pub frame: Option<String>,
    pub object: Option<String>,
    /// `base-uri`: what a `<base>` may point the page's relative URLs at.
    pub base: Option<String>,
    /// `form-action`: where a form may submit.
    pub form: Option<String>,
    /// `report-uri`: where a violation report is posted.
    pub report: Option<String>,
}

/// Linting of the built pages: which rules run over the typed DOM, how loud a
/// finding is, and how many bytes a page may weigh.
///
/// Off until a `lint { }` block says otherwise. A lint is a claim about what the
/// site *should* look like, and inventing one for a site that never asked is the
/// same opinionated-default problem as a generated page nobody wanted.
#[derive(Debug, Clone, Hash)]
pub struct LintConfig {
    /// Whether the DOM lint pass runs at all; flipped by the block's presence.
    pub enabled: bool,
    /// Fail the build on a finding instead of warning, exactly as
    /// [`LinkConfig::strict`] does for a broken link.
    pub strict: bool,
    /// Report a heading that skips a level (`h2` straight to `h4`).
    pub headings: bool,
    /// Report an `<img>` carrying no `alt` (an empty one is a decorative image,
    /// and is fine).
    pub alt: bool,
    /// Report an `id` used more than once on one page.
    pub ids: bool,
    /// Report an unknown ARIA role or `aria-*` attribute, and one whose id
    /// reference names nothing on the page.
    pub aria: bool,
    /// How many bytes a single page may ship.
    pub budget: BudgetConfig,
}

/// Per-page weight limits, in bytes. Each is the ceiling for one class of what
/// a page ships; `None` is no limit.
///
/// Unlike the rules above, a budget always fails the build: it is an assertion
/// the author wrote down, not an opinion this tool holds.
#[derive(Debug, Clone, Default, Hash)]
pub struct BudgetConfig {
    /// The page's own markup, as written to `dist`.
    pub html: Option<Bytes>,
    /// Every script the page loads, plus its inline `<script>` bodies.
    pub js: Option<Bytes>,
    /// Every stylesheet it loads, plus its inline `<style>` bodies.
    pub css: Option<Bytes>,
    /// Every image it references, responsive candidates excluded: a `srcset`
    /// offers alternatives, and a visitor is served one of them.
    pub images: Option<Bytes>,
    /// All of the above at once, the page's total transfer weight.
    pub total: Option<Bytes>,
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
    /// whole site. Follows the term pages, so it needs `listing` on the
    /// taxonomy.
    pub terms: bool,
    /// What each format's file is called, when the conventional name is not the
    /// one a site already publishes under. A moved feed is the one move a
    /// redirect stub cannot rescue, since a reader fetches the file and never
    /// renders the meta refresh.
    pub names: FeedNames,
}

/// Per-format file name overrides for [`FeedConfig`].
#[derive(Debug, Clone, Default, Hash)]
pub struct FeedNames {
    pub rss: Option<String>,
    pub atom: Option<String>,
    pub json: Option<String>,
}

impl FeedConfig {
    /// This format's file name: the configured override, else the conventional
    /// one.
    ///
    /// The single answer, because a feed names its own file in three places and
    /// an aggregator would notice them disagreeing: the file the build writes,
    /// the `<id>`/`feed_url` inside it, and every page's autodiscovery tag.
    pub fn file(&self, kind: FeedKind) -> &str {
        let named = match kind {
            FeedKind::Rss => &self.names.rss,
            FeedKind::Atom => &self.names.atom,
            FeedKind::Json => &self.names.json,
        };
        named.as_deref().unwrap_or_else(|| kind.file())
    }

    /// This feed's absolute URL under `base`, for a language `scope` (empty for
    /// the default language).
    ///
    /// The file name is appended to the scope's directory URL rather than
    /// joined as a path segment, which would give it a trailing slash.
    pub fn url(&self, kind: FeedKind, base: &BaseUrl, scope: &str) -> String {
        format!(
            "{}{}",
            base.join(Permalink::join(&[scope])),
            self.file(kind)
        )
    }
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
    /// The conventional output file name for this format, which
    /// [`FeedConfig::file`] overrides.
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
    /// Also emit the shipped search UI (a Ctrl-K palette) next to each index.
    /// Spelled `ui` in config: the top-level `client { }` block is build-time
    /// constants for client JS, and one name could not mean both.
    pub ui: bool,
}

impl SearchConfig {
    /// Whether the prebuilt inverted index is among the formats emitted. It is
    /// the only one `stopwords` and `minimum` reach, so it is also what decides
    /// whether either of them does anything.
    pub fn inverted(&self) -> bool {
        self.formats.contains(&SearchFormat::Inverted)
    }
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
    /// Emit a schema.org JSON-LD island in each page's `<head>`.
    ///
    /// Opt-in, unlike the meta tags beside it: those restate facts the page
    /// already states, while structured data is a claim made *to* a search
    /// engine about what the page is, and that is the author's claim to make.
    pub jsonld: bool,
    /// Where a page's footnotes are moved to.
    pub footnotes: Footnotes,
    /// Stamp every element with the `file:line:column` it was authored at, as
    /// `data-typst`. What a source-mapped preview reads to jump from a rendered
    /// element back to the Typst that produced it.
    ///
    /// Opt-in, and off in a published build: the attributes are for the author,
    /// not the reader. `serve --spans` turns them on for a preview session.
    /// Deliberately a config field rather than a `serve`-only flag: `serve`
    /// settings are excluded from the cache fingerprint, so a mode-derived
    /// stamp would leave a `build` reusing a served page's markup, attributes
    /// and all.
    pub spans: bool,
}

/// The elements a page's footnote list is moved into, most specific first.
///
/// Typst appends the list to the end of the document, which is right for a page
/// with no template and wrong for one with a layout: everything the layout emits
/// is already in the body, so the notes land after the site footer, outside the
/// element that sets the content width.
///
/// This is a list rather than one name because a site's layouts rarely agree: a
/// post wraps its body in `<article>`, a generated index has only `<main>`, and
/// a bespoke page may have neither. Each name is tried in order and the first
/// element found wins, so `footnotes "article" "main"` covers all three without
/// a rule per template. An empty list moves nothing, which is how a site keeps
/// Typst's own placement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Footnotes(Vec<String>);

impl Default for Footnotes {
    /// An article, else the main region: the two elements a layout is most
    /// likely to have, in the order that puts the notes closest to the text
    /// they annotate.
    fn default() -> Self {
        Self(vec!["article".to_owned(), "main".to_owned()])
    }
}

impl Footnotes {
    /// The element names to try, in order.
    pub fn targets(&self) -> &[String] {
        &self.0
    }

    /// Whether the notes stay where Typst put them.
    pub fn disabled(&self) -> bool {
        self.0.is_empty()
    }
}

/// Built from the configured names, which the parser has already checked are
/// element names the DOM can hold.
impl From<Vec<String>> for Footnotes {
    fn from(names: Vec<String>) -> Self {
        Self(names)
    }
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
        format!(
            "sx-{}",
            named.unwrap_or_else(|| colour.trim_start_matches('#'))
        )
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
    pub(crate) const MAX: u32 = 4096;

    /// The served URL of a page's card, whether or not it has been rendered
    /// yet: the meta transform names it while the file is still being made, the
    /// renderer writes it, and the prune keeps it, so all three have to derive
    /// it the same way.
    pub fn url(&self, permalink: &str) -> String {
        format!("/{}/{}.png", Self::DIR, Basename(permalink))
    }

    /// Whether cards are actually produced: configured *and* compiled in. A
    /// build without the `cards` feature has no rasterizer, so pointing pages at
    /// images it cannot make would be worse than making none.
    pub fn active(&self) -> bool {
        self.enabled && cfg!(feature = "cards")
    }
}

/// What the typesetter writes on paper: `generate { pdf { .. } }`.
///
/// The other half of what it can do with the same source: the HTML compile
/// targets a DOM, these target pages. Two artifacts, switched on separately by
/// the presence of their own block, because wanting one says nothing about
/// wanting the other: a manual is bundled and rarely per-page, a blog is the
/// reverse.
#[derive(Debug, Clone, Hash, Default)]
pub struct PdfConfig {
    /// One PDF per page, beside its HTML.
    pub pages: PdfPages,
    /// Many pages as one document.
    pub bundle: PdfBundle,
}

impl PdfConfig {
    /// Whether the site asked for either artifact, for the feature gate: a
    /// binary without the exporter has to say so whichever one was asked for.
    pub fn enabled(&self) -> bool {
        self.pages.enabled || self.bundle.enabled()
    }
}

/// One PDF per page, from a paged template. Enabled by the presence of a
/// `generate { pdf { pages { .. } } }` block.
///
/// Like a card it needs its own template, because a layout that emits
/// `html.elem` produces nothing on the paged target.
#[derive(Debug, Clone, Hash)]
pub struct PdfPages {
    /// Whether to write a PDF per page.
    pub enabled: bool,
    /// The paged template file under the templates directory.
    pub template: String,
}

impl PdfPages {
    /// The served URL of a page's PDF: a sibling of the page rather than a file
    /// inside it, so `/posts/hello/` yields `/posts/hello.pdf` and a browser
    /// saves it under a name that means something. `/posts/hello/index.pdf`
    /// would download as `index.pdf`.
    pub fn url(&self, permalink: &str) -> String {
        format!("/{}.pdf", Basename(permalink))
    }

    /// Whether per-page PDFs are actually produced: configured *and* compiled
    /// in. A build without the `pdf` feature has no exporter, so linking pages
    /// to a file it cannot make would be worse than making none.
    pub fn active(&self) -> bool {
        self.enabled && cfg!(feature = "pdf")
    }
}

/// Many pages as one document: a collection bound end to end, the whole site,
/// or both. Enabled by the presence of a `generate { pdf { bundle { .. } } }`
/// block naming at least one target.
///
/// The paged sibling of `navigation { standalone }`, which does the same thing
/// for HTML.
#[derive(Debug, Clone, Hash)]
pub struct PdfBundle {
    /// Whether the site wrote a `bundle { }` block at all, as distinct from
    /// having named a target in one. An empty block asks for nothing, and the
    /// difference is what lets the build say so instead of writing no file in
    /// silence.
    pub present: bool,
    /// The paged template file under the templates directory. Distinct from the
    /// per-page one: it is handed every page at once, and what it does with a
    /// run of documents (a title page, a contents list, running heads) is not
    /// what a single page's template does.
    pub template: String,
    /// Collections to bundle, each written to `/<collection>.pdf`.
    pub collections: Vec<String>,
    /// Whether to bundle the whole site, written to `/site.pdf`. Named by the
    /// same rule its neighbours are: a bundle is `/<target>.pdf`, and inventing
    /// a second rule so this one could carry a filename bought nothing.
    pub site: bool,
}

impl PdfBundle {
    /// Whether any target was named. A `bundle { }` block that names none asks
    /// for nothing, which [`crate::engine`]'s inert-setting table reports
    /// rather than letting the build write nothing in silence.
    pub fn enabled(&self) -> bool {
        !self.collections.is_empty() || self.site
    }

    /// Whether bundles are actually produced: asked for *and* compiled in.
    pub fn active(&self) -> bool {
        self.enabled() && cfg!(feature = "pdf")
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

    /// Every spelling this enum accepts, in declaration order: what the
    /// generated reference lists for a `Kind::Choice` key, read out of the same
    /// table that parses them.
    fn names() -> Vec<&'static str> {
        Self::NAMES.iter().map(|(name, _)| *name).collect()
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

impl ResponsiveConfig {
    /// The widths worth emitting for a source that is `source` pixels wide:
    /// the configured ones below it, deduped and ascending. A width at or above
    /// the source is skipped rather than upscaled, and the source itself is the
    /// largest candidate, so it is not in this list.
    ///
    /// One rule, because two layers apply it and must agree on the answer: the
    /// asset pipeline generates the files, and the render pass names them in a
    /// `srcset` before an extracted image has any.
    pub fn applicable(&self, source: u32) -> Vec<u32> {
        let mut widths: Vec<u32> = self
            .widths
            .iter()
            .copied()
            .filter(|&w| w < source)
            .collect();
        widths.sort_unstable();
        widths.dedup();
        widths
    }
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

/// `manifest.webmanifest` generation ([the web app manifest][spec]): what a
/// browser reads when a visitor installs the site to a home screen. Enabled by
/// the presence of a `generate { manifest }` block.
///
/// Everything here is what only the author knows. What the build already knows
/// (the site title, the language's root URL) is filled in from the config it is
/// written for, so a bare `manifest { }` beside an icon is a valid manifest.
///
/// [spec]: https://www.w3.org/TR/appmanifest/
#[derive(Debug, Clone, Hash, Default)]
pub struct ManifestConfig {
    /// Whether to emit `manifest.webmanifest`.
    pub enabled: bool,
    /// The installed app's name. Defaults to the site title in the language the
    /// manifest is written for.
    pub name: Option<String>,
    /// The name a launcher falls back to when the full one does not fit.
    pub short: Option<String>,
    /// One line about the app, shown by an install prompt.
    pub description: Option<String>,
    /// How the installed app is presented.
    pub display: DisplayMode,
    /// CSS colour of the browser UI around the app, also written to every
    /// page's `<meta name="theme-color">` so a tab is tinted before any install.
    pub theme: Option<String>,
    /// CSS colour painted before the first page has rendered.
    pub background: Option<String>,
    /// Where launching the installed app lands, as a root-relative path.
    /// Localized per language, like the default it replaces: `/home/` launches
    /// the French app into `/fr/home/`. Defaults to the language's root.
    pub start: Option<String>,
    /// The URLs the installed app covers; navigating outside it leaves the app.
    /// Localized the same way, since a `start_url` outside its `scope` is a
    /// manifest a browser refuses. Defaults to the language's root.
    pub scope: Option<String>,
    /// The icons a launcher picks from. A manifest with none cannot be
    /// installed, so a build that emits one warns.
    pub icons: Vec<IconConfig>,
}

impl ManifestConfig {
    /// The output file name, at the root of each language's scope. The
    /// `.webmanifest` extension is the one the spec registers; `manifest.json`
    /// is the older spelling, and browsers accept both.
    pub const FILE: &'static str = "manifest.webmanifest";

    /// The manifest of a language, root-relative and under the site's base
    /// path: what that language's pages point `<link rel="manifest">` at.
    ///
    /// Beside [`FILE`](Self::FILE) for the reason [`FeedKind::url`] sits beside
    /// its file name: the processor that writes the file and the tag that names
    /// it derive the path once, so they cannot drift. Root-relative rather than
    /// absolute, so a manifest is reachable without a configured site `url`.
    pub fn url(config: &Config, lang: &str) -> String {
        let scope = config.scope(lang, "");
        let path = match scope.is_empty() {
            true => format!("/{}", Self::FILE),
            false => format!("/{scope}/{}", Self::FILE),
        };
        config.prefixed(&path)
    }
}

/// One entry of a manifest's `icons` array.
#[derive(Debug, Clone, Hash)]
pub struct IconConfig {
    /// Where the image is served from, root-relative, exactly as a browser will
    /// request it. Written as the node's name: `"/icon-512.png" size=512`.
    pub src: String,
    /// The square edge in pixels. Absent means the image scales to any size,
    /// which is what a vector icon does.
    pub size: Option<u32>,
    /// What a launcher may do with the image.
    pub purpose: IconPurpose,
}

/// An icon is written as a path with its dimensions attached, so the path is
/// what one is built from and the rest is filled in from the line's attributes.
impl From<String> for IconConfig {
    fn from(src: String) -> Self {
        Self {
            src,
            size: None,
            purpose: IconPurpose::default(),
        }
    }
}

/// How an installed app is presented, [as the manifest spells it][spec].
///
/// [spec]: https://www.w3.org/TR/appmanifest/#display-member
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DisplayMode {
    /// Its own window, with no browser UI. Why a site ships a manifest at all,
    /// hence the default.
    #[default]
    Standalone,
    /// Its own window, and the whole screen.
    Fullscreen,
    /// Its own window, keeping the minimum navigation UI the browser insists on.
    Minimal,
    /// An ordinary browser tab.
    Browser,
}

impl Named for DisplayMode {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("standalone", Self::Standalone),
        ("fullscreen", Self::Fullscreen),
        ("minimal", Self::Minimal),
        ("browser", Self::Browser),
    ];
}

impl DisplayMode {
    /// The spelling the manifest takes, which is the config spelling bar
    /// `minimal`: the member is `minimal-ui`, and a config key or value is one
    /// word.
    pub fn member(self) -> &'static str {
        match self {
            Self::Minimal => "minimal-ui",
            other => other.name(),
        }
    }
}

/// What a launcher may do with an icon, [as the manifest spells it][spec].
///
/// [spec]: https://www.w3.org/TR/appmanifest/#purpose-member
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IconPurpose {
    /// Shown as drawn, whatever the platform's icon shape is.
    #[default]
    Any,
    /// Safe to crop to the platform's shape: the image keeps its subject inside
    /// the safe zone and fills the rest with its own background.
    Maskable,
    /// A single-colour glyph the platform recolours, for a notification badge.
    Monochrome,
}

impl Named for IconPurpose {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("any", Self::Any),
        ("maskable", Self::Maskable),
        ("monochrome", Self::Monochrome),
    ];
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
    /// The `tsconfig.json` the bundler transforms TypeScript and JSX against,
    /// relative to the project root. `None` means the bundler discovers one per
    /// module, walking up from the file as `tsc` does; a path pins the whole
    /// site to one file, wherever the scripts live.
    pub tsconfig: Option<PathBuf>,
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
    /// Region code, resolved by [`S3Config::region`] when unset.
    pub region: Option<String>,
    /// Key prefix every uploaded object is placed under (a subdirectory in the
    /// bucket). Empty by default.
    pub prefix: String,
    /// Delete remote objects under `prefix` that the build no longer produces.
    pub delete: bool,
}

impl S3Config {
    /// The region code the request is signed under.
    ///
    /// A stated `region` always wins. Otherwise it follows the target: AWS is
    /// signed as `us-east-1`, its own default, and a custom `endpoint` is not
    /// AWS, so it is signed as `auto`, which is what R2 and most S3-compatible
    /// hosts want. Defaulting the second case to `us-east-1` meant an R2 user
    /// who set `endpoint` and left `region` alone got a 403 with nothing in it
    /// naming the region.
    pub fn region(&self) -> &str {
        match (&self.region, &self.endpoint) {
            (Some(region), _) => region,
            (None, Some(_)) => "auto",
            (None, None) => "us-east-1",
        }
    }
}

/// The `Cache-Control` an uploaded object is served with, and the reason
/// `assets { fingerprint }` is worth turning on.
///
/// Fingerprinting renames a file after its own content, which makes it safe to
/// cache forever: a change produces a different name, so a stale copy is never
/// the one asked for. A raw bucket sets no `Cache-Control` at all, though, and
/// Netlify/Vercel/Cloudflare Pages only guess. Without this the whole point of
/// hashing a filename is thrown away at the last step.
///
/// Enabled by the presence of a `caching { }` block; both values have defaults, so
/// a bare `cache` is the sensible policy.
#[derive(Debug, Clone, Hash, Default)]
pub struct CacheControl {
    /// Whether to send `Cache-Control` at all.
    pub enabled: bool,
    /// For content-addressed files: everything under the asset prefix, once
    /// `assets { fingerprint }` is on. Cached indefinitely.
    pub immutable: String,
    /// For everything else: pages, feeds, `robots.txt`, and any asset whose name
    /// is not a hash. Revalidated, because these keep their names across builds.
    pub default: String,
}

impl CacheControl {
    /// The header value for `key`, or `None` when no policy is configured.
    ///
    /// `hashed` says whether this build content-addresses its assets; without
    /// it, a file under the asset prefix keeps its authored name across builds
    /// and is exactly as mutable as a page.
    pub fn header(&self, key: &str, prefix: &str, hashed: bool) -> Option<&str> {
        if !self.enabled {
            return None;
        }
        let immutable = hashed && key.trim_start_matches('/').starts_with(prefix);
        Some(match immutable {
            true => &self.immutable,
            false => &self.default,
        })
    }
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
    /// The command that opens a source location, run when a preview alt-click
    /// asks for one. The program first, then each argument as its own word,
    /// with `{file}`, `{line}` and `{column}` substituted per argument: no
    /// shell, so a path is never re-parsed as a command line.
    ///
    /// Empty means no editor, and the preview says so rather than guessing at
    /// one.
    pub editor: Vec<String>,
}
