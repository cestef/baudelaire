//! The config schema: every KDL key paired with the handler that applies it.
//!
//! Each section carries its own table by implementing [`Section`] (a `{ .. }`
//! block) or [`Attributed`] (a `key=value` line), so a scope's valid keys, its
//! parsing, and its "unknown key" suggestions all read one list. The typed
//! primitives those handlers are written in terms of live in [`super::node`] and
//! [`super::value`].

use kdl::{KdlDocument, KdlNode};

use crate::config::dispatch::{Attributed, Attrs, Block, Section};
use crate::config::node::NodeExt;
use crate::config::permalink::Permalink;
use crate::config::value::ValueExt;
use crate::config::{
    AnnounceConfig, AssetConfig, CacheConfig, CacheControl, CardsConfig, CollectionConfig, Config,
    ContentConfig, DeployConfig, DraftConfig, Eagerness, FeedConfig, FeedKind, GenerateConfig,
    HooksConfig, HtmlConfig, ImagesConfig, JpegConfig, LanguageConfig, LinkConfig, LlmsConfig,
    NavigationConfig, OptimizeConfig, PaginateConfig, Paths, PngConfig, PngStrip, Prefetch,
    ResponsiveConfig, RobotsConfig, Router, S3Config, SearchConfig, SearchField, SearchFormat,
    ServeConfig, SortKey, SpaConfig, SpeculationConfig, SshConfig, StandaloneConfig,
    StandardConfig, TaxonomyConfig, TypstConfig, UrlStyle, VerifyConfig,
};
use crate::error::{ConfigError, ConfigErrorKind, Result};

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        // keep the raw text: profile overlay reports errors against it, its nodes carry spans into it
        let mut cfg = Config {
            source: text.to_owned(),
            ..Config::default()
        };
        cfg.apply(doc.nodes(), text)?;
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
        ("site", |c, n, t| {
            c.site = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("url", |c, n, t| {
            c.url = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("lang", |c, n, t| {
            c.lang = n.string(t, 0)?;
            Ok(())
        }),
        ("author", |c, n, t| {
            c.author = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("theme", |c, n, t| {
            c.theme = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("paths", |c, n, t| c.paths.fill(n, t)),
        ("content", |c, n, t| c.content.fill(n, t)),
        ("languages", |c, n, t| {
            c.languages = n.unique(t, "language", LanguageConfig::item)?;
            Ok(())
        }),
        ("assets", |c, n, t| c.assets.fill(n, t)),
        ("html", |c, n, t| c.html.fill(n, t)),
        ("links", |c, n, t| c.links.fill(n, t)),
        ("generate", |c, n, t| c.generate.fill(n, t)),
        ("navigation", |c, n, t| c.navigation.fill(n, t)),
        ("prune", |c, n, t| {
            c.prune = n.boolean(t, 0)?;
            Ok(())
        }),
        ("cache", |c, n, t| c.cache.fill(n, t)),
        ("caching", |c, n, t| c.caching.fill(n, t)),
        ("typst", |c, n, t| c.typst.fill(n, t)),
        ("client", |c, n, t| {
            c.client = n.table(t)?;
            Ok(())
        }),
        ("hooks", |c, n, t| c.hooks.fill(n, t)),
        ("announce", |c, n, t| c.announce.fill(n, t)),
        ("deploy", |c, n, t| c.deploy.fill(n, t)),
        ("serve", |c, n, t| c.serve.fill(n, t)),
        (Config::PROFILES, |c, n, t| {
            c.profiles = n.unique(t, "profile", |child, t| {
                Ok((child.name().value().to_owned(), child.block(t)?.clone()))
            })?;
            Ok(())
        }),
    ]);
}

/// The `paths { .. }` section: directory layout knobs.
impl Section for Paths {
    const RULES: Block<Self> = Block(&[
        ("content", |c, n, t| {
            c.content = n.string(t, 0)?.into();
            Ok(())
        }),
        ("dist", |c, n, t| {
            c.dist = n.string(t, 0)?.into();
            Ok(())
        }),
        ("assets", |c, n, t| {
            c.assets = n.string(t, 0)?.into();
            Ok(())
        }),
        ("static", |c, n, t| {
            c.r#static = n.string(t, 0)?.into();
            Ok(())
        }),
        ("templates", |c, n, t| {
            c.templates = n.string(t, 0)?.into();
            Ok(())
        }),
    ]);
}

/// The `content { .. }` section: what the content tree holds and how it is
/// read. The directory it lives in is `paths { content }`.
impl Section for ContentConfig {
    const RULES: Block<Self> = Block(&[
        ("index", |c, n, t| {
            let stem = n.string(t, 0)?;
            // A stem, matched against `Stem::slug`, which never carries an
            // extension. `index "index.typ"` therefore matches nothing: every
            // bundle keeps its filename slug, `content/index.typ` publishes to
            // `/index/`, and the site builds green with no home page at all.
            // The docs shipped that exact spelling, so refuse it by name.
            if let Some(stem) = stem.strip_suffix(".typ").filter(|s| !s.is_empty()) {
                return Err(ConfigError::at(
                    t,
                    ConfigErrorKind::IndexExtension {
                        got: n.string(t, 0)?,
                        stem: stem.to_owned(),
                    },
                    n.span(),
                )
                .into());
            }
            c.index = (!stem.is_empty()).then_some(stem);
            Ok(())
        }),
        ("future", |c, n, t| {
            c.future = n.boolean(t, 0)?;
            Ok(())
        }),
        ("draft", |c, n, t| c.draft.fill(n, t)),
        ("collections", |c, n, t| {
            c.collections = n.unique(t, "collection", CollectionConfig::item)?;
            Ok(())
        }),
        ("taxonomies", |c, n, t| {
            c.taxonomies = n.unique(t, "taxonomy", TaxonomyConfig::item)?;
            Ok(())
        }),
    ]);
}

impl Section for DraftConfig {
    const RULES: Block<Self> = Block(&[
        ("build", |c, n, t| {
            c.build = n.boolean(t, 0)?;
            Ok(())
        }),
        ("suffix", |c, n, t| {
            c.suffix = n.string(t, 0)?;
            Ok(())
        }),
    ]);
}

impl CollectionConfig {
    /// One `posts { .. }` block: the node name is the collection id, and an
    /// optional leading positional its member glob, so the one-line
    /// `posts "posts/**/*.typ"` shorthand still reads.
    fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let mut cfg = Self::default();
        // At most the glob. Anything after it would be silently discarded, and
        // a setting that looks accepted and does nothing is the failure this
        // whole dispatch layer exists to prevent.
        for (position, entry) in node.entries().iter().enumerate() {
            match position {
                0 => cfg.glob = Some(entry.value().as_str(text, NodeExt::span(node))?),
                _ => {
                    return Err(ConfigError::unexpected_argument(
                        text,
                        &entry.value().to_string(),
                        node.name().value(),
                        crate::config::node::EntryExt::span(entry),
                    )
                    .into());
                }
            }
        }
        // A bare `posts` or `posts "glob"` is a collection that only declares
        // its members; everything else it could say lives in the block.
        if node.children().is_some() {
            cfg.fill(node, text)?;
        }
        Ok((node.name().value().to_owned(), cfg))
    }
}

/// One collection: what belongs to it, how its members are ordered and
/// addressed, and (in a nested block) the index generated over them.
impl Section for CollectionConfig {
    const RULES: Block<Self> = Block(&[
        ("glob", |c, n, t| {
            c.glob = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("sort", |c, n, t| {
            c.sort = n.arg(t, 0)?.one::<SortKey>(t, NodeExt::span(n))?;
            Ok(())
        }),
        ("reverse", |c, n, t| {
            c.reverse = n.boolean(t, 0)?;
            Ok(())
        }),
        ("permalink", |c, n, t| {
            let raw = n.string(t, 0)?;
            // validate here so a template typo is a spanned config error,
            // not a silent fallback to convention at page load
            Permalink::parse(&raw).map_err(|e| ConfigError::at(t, e.into(), NodeExt::span(n)))?;
            c.permalink = Some(raw);
            Ok(())
        }),
        ("template", |c, n, t| {
            c.template = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("paginate", |c, n, t| c.paginate.fill(n, t)),
    ]);
}

/// The `paginate { .. }` block inside a collection: its presence generates the
/// index, and every key tunes it.
impl Section for PaginateConfig {
    const RULES: Block<Self> = Block(&[
        ("size", |c, n, t| {
            let n_ = n.arg(t, 0)?.integer(t, NodeExt::span(n))?;
            if n_ < 1 {
                return Err(ConfigError::paginate_too_small(t, n_, NodeExt::span(n)).into());
            }
            c.size = Some(n_ as usize);
            Ok(())
        }),
        ("template", |c, n, t| {
            c.template = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("mount", |c, n, t| {
            c.mount = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("prefix", |c, n, t| {
            c.prefix = n.string(t, 0)?;
            Ok(())
        }),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

impl TaxonomyConfig {
    /// One `tags key=.. listing=..` line, defaulting to the frontmatter key
    /// that shares the taxonomy's id.
    fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let id = node.name().value().to_owned();
        let mut taxonomy = Self::from(id.clone());
        taxonomy.read(node, text)?;
        Ok((id, taxonomy))
    }
}

impl Attributed for TaxonomyConfig {
    const ATTRS: Attrs<Self> = Attrs(&[
        ("key", |c, v, t, s| {
            c.key = v.as_str(t, s)?;
            Ok(())
        }),
        ("listing", |c, v, t, s| {
            c.listing = v.boolean(t, s)?;
            Ok(())
        }),
        ("template", |c, v, t, s| {
            c.template = Some(v.as_str(t, s)?);
            Ok(())
        }),
        ("paginate", |c, v, t, s| {
            let n = v.integer(t, s)?;
            if n < 1 {
                return Err(ConfigError::paginate_too_small(t, n, s).into());
            }
            c.paginate = Some(n as usize);
            Ok(())
        }),
        ("prefix", |c, v, t, s| {
            c.prefix = v.as_str(t, s)?;
            Ok(())
        }),
    ]);
}

impl LanguageConfig {
    /// One declared language, keyed by its code.
    fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let mut lang = Self::default();
        lang.fill(node, text)?;
        Ok((node.name().value().to_owned(), lang))
    }
}

/// Scalar fields dispatch like any other section; the nested `strings { .. }`
/// table reuses the very parser `client { .. }` uses, so a UI string dictionary
/// and a client-constant block stay one shape.
impl Section for LanguageConfig {
    const RULES: Block<Self> = Block(&[
        ("name", |c, n, t| {
            c.name = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("dir", |c, n, t| {
            c.dir = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("site", |c, n, t| {
            c.site = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("author", |c, n, t| {
            c.author = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("strings", |c, n, t| {
            c.strings = n.table(t)?;
            Ok(())
        }),
    ]);
}

/// The `assets { .. }` section: the pipeline applied to `paths { assets }`.
impl Section for AssetConfig {
    const RULES: Block<Self> = Block(&[
        ("minify", |c, n, t| {
            c.minify = n.boolean(t, 0)?;
            Ok(())
        }),
        ("bundle", |c, n, t| {
            c.bundle = n.boolean(t, 0)?;
            Ok(())
        }),
        ("fingerprint", |c, n, t| {
            c.fingerprint = n.boolean(t, 0)?;
            Ok(())
        }),
        ("images", |c, n, t| c.images.fill(n, t)),
    ]);
}

/// The `images { .. }` section: markup annotations and build-time processing.
impl Section for ImagesConfig {
    const RULES: Block<Self> = Block(&[
        ("lazy", |c, n, t| {
            c.lazy = n.boolean(t, 0)?;
            Ok(())
        }),
        ("extract", |c, n, t| {
            c.extract = n.boolean(t, 0)?;
            Ok(())
        }),
        ("optimize", |c, n, t| c.optimize.fill(n, t)),
        ("responsive", |c, n, t| c.responsive.fill(n, t)),
    ]);
}

/// The `responsive { widths .. ; quality N }` block. Its presence enables
/// width-variant generation; widths and quality keep their defaults unless
/// named.
impl Section for ResponsiveConfig {
    const RULES: Block<Self> = Block(&[
        ("widths", |c, n, t| {
            c.widths = n.widths(t)?;
            Ok(())
        }),
        ("quality", |c, n, t| {
            c.quality = n.arg(t, 0)?.ranged(t, NodeExt::span(n), 1, 100)? as u8;
            Ok(())
        }),
        ("sizes", |c, n, t| {
            c.sizes = Some(n.string(t, 0)?);
            Ok(())
        }),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `optimize { png [level=..] [strip=..]; jpeg [quality=..] }` block: each
/// child names a format and enables it, with optional per-format tuning as
/// attributes. Fills onto the existing per-format config so a profile tuning
/// one attribute keeps its siblings.
///
/// One spelling per format. `jpg` used to be a second key onto the same field,
/// so `optimize { jpeg quality=70; jpg quality=90 }` parsed as one format
/// configured twice with the last winning, and the "valid keys" help offered
/// both as if they were different formats.
impl Section for OptimizeConfig {
    const RULES: Block<Self> = Block(&[
        ("png", |c, n, t| c.png.get_or_insert_default().read(n, t)),
        ("jpeg", |c, n, t| c.jpeg.get_or_insert_default().read(n, t)),
    ]);
}

impl Attributed for PngConfig {
    const ATTRS: Attrs<Self> = Attrs(&[
        ("level", |c, v, t, s| {
            c.level = v.ranged(t, s, 0, 6)? as u8;
            Ok(())
        }),
        ("strip", |c, v, t, s| {
            c.strip = v.one::<PngStrip>(t, s)?;
            Ok(())
        }),
    ]);
}

impl Attributed for JpegConfig {
    const ATTRS: Attrs<Self> = Attrs(&[("quality", |c, v, t, s| {
        c.quality = v.ranged(t, s, 1, 100)? as u8;
        Ok(())
    })]);
}

/// The `html { .. }` section: post-processing of typst's HTML output.
impl Section for HtmlConfig {
    const RULES: Block<Self> = Block(&[
        ("pretty", |c, n, t| {
            c.pretty = n.boolean(t, 0)?;
            Ok(())
        }),
        ("embed", |c, n, t| {
            c.embed = n.boolean(t, 0)?;
            Ok(())
        }),
        ("meta", |c, n, t| {
            c.meta = n.boolean(t, 0)?;
            Ok(())
        }),
        ("anchors", |c, n, t| {
            c.anchors = n.boolean(t, 0)?;
            Ok(())
        }),
        ("jsonld", |c, n, t| {
            c.jsonld = n.boolean(t, 0)?;
            Ok(())
        }),
        ("highlight", |c, n, t| {
            c.highlight.enabled = true;
            // A bare `highlight` rewrites every colour to its hex class; a block
            // names the scopes the theme paints, so the classes read as meaning
            // rather than as colours.
            if n.children().is_some() {
                c.highlight.scopes = n.pairs(t)?;
            }
            Ok(())
        }),
    ]);
}

/// The `links { .. }` section: URL shape and link checking.
impl Section for LinkConfig {
    const RULES: Block<Self> = Block(&[
        ("style", |c, n, t| {
            c.style = n.arg(t, 0)?.one::<UrlStyle>(t, NodeExt::span(n))?;
            Ok(())
        }),
        ("strict", |c, n, t| {
            c.strict = n.boolean(t, 0)?;
            Ok(())
        }),
        ("external", |c, n, t| {
            c.external = n.boolean(t, 0)?;
            Ok(())
        }),
    ]);
}

/// The `generate { .. }` section: the files a build emits beside the pages.
/// Each child is opt-in, either a flag or a block whose presence turns it on.
impl Section for GenerateConfig {
    const RULES: Block<Self> = Block(&[
        ("sitemap", |c, n, t| {
            c.sitemap = n.boolean(t, 0)?;
            Ok(())
        }),
        ("redirects", |c, n, t| {
            c.redirects = n.boolean(t, 0)?;
            Ok(())
        }),
        ("headers", |c, n, t| {
            c.headers = n.boolean(t, 0)?;
            Ok(())
        }),
        ("robots", |c, n, t| c.robots.fill(n, t)),
        ("llms", |c, n, t| c.llms.fill(n, t)),
        ("feed", |c, n, t| c.feed.fill(n, t)),
        ("search", |c, n, t| c.search.fill(n, t)),
        ("cards", |c, n, t| c.cards.fill(n, t)),
    ]);
}

impl Section for RobotsConfig {
    const RULES: Block<Self> = Block(&[("disallow", |c, n, t| {
        c.disallow = n.words(t)?;
        Ok(())
    })]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

impl Section for LlmsConfig {
    const RULES: Block<Self> = Block(&[("summary", |c, n, t| {
        c.summary = Some(n.string(t, 0)?);
        Ok(())
    })]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

impl Section for FeedConfig {
    const RULES: Block<Self> = Block(&[
        ("formats", |c, n, t| {
            c.formats = n.mapped::<FeedKind>(t)?;
            Ok(())
        }),
        ("limit", |c, n, t| {
            c.limit = n.count(t, 0)?;
            Ok(())
        }),
        ("terms", |c, n, t| {
            c.terms = n.boolean(t, 0)?;
            Ok(())
        }),
    ]);
}

impl Section for SearchConfig {
    const RULES: Block<Self> = Block(&[
        ("formats", |c, n, t| {
            c.formats = n.mapped::<SearchFormat>(t)?;
            Ok(())
        }),
        ("fields", |c, n, t| {
            c.fields = n.mapped::<SearchField>(t)?;
            Ok(())
        }),
        ("stopwords", |c, n, t| {
            c.stopwords = n.words(t)?;
            Ok(())
        }),
        ("minimum", |c, n, t| {
            c.min_length = n.count(t, 0)?;
            Ok(())
        }),
        ("ui", |c, n, t| {
            c.ui = n.boolean(t, 0)?;
            Ok(())
        }),
    ]);
}

/// The `cards { template ..; width ..; height .. }` block. Its presence enables
/// social card rendering.
impl Section for CardsConfig {
    const RULES: Block<Self> = Block(&[
        ("template", |c, n, t| {
            c.template = n.string(t, 0)?;
            Ok(())
        }),
        ("width", |c, n, t| {
            c.width = n
                .arg(t, 0)?
                .ranged(t, NodeExt::span(n), 1, CardsConfig::MAX)? as u32;
            Ok(())
        }),
        ("height", |c, n, t| {
            c.height = n
                .arg(t, 0)?
                .ranged(t, NodeExt::span(n), 1, CardsConfig::MAX)? as u32;
            Ok(())
        }),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `navigation { .. }` section: how a visitor moves between the built
/// pages. Each child block's presence enables that strategy.
impl Section for NavigationConfig {
    const RULES: Block<Self> = Block(&[
        ("spa", |c, n, t| c.spa.fill(n, t)),
        ("standalone", |c, n, t| c.standalone.fill(n, t)),
        ("speculation", |c, n, t| c.speculation.fill(n, t)),
    ]);
}

/// The `spa { root ..; prefetch .. }` block. Its presence enables the
/// client-side navigation runtime.
impl Section for SpaConfig {
    const RULES: Block<Self> = Block(&[
        ("root", |c, n, t| {
            c.root = n.string(t, 0)?;
            Ok(())
        }),
        ("prefetch", |c, n, t| {
            c.prefetch = n.arg(t, 0)?.one::<Prefetch>(t, NodeExt::span(n))?;
            Ok(())
        }),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `standalone { file ..; entry ..; router .. }` block. Its presence
/// enables the single-file export; the rest keep their defaults unless named.
impl Section for StandaloneConfig {
    const RULES: Block<Self> = Block(&[
        ("file", |c, n, t| {
            c.file = n.contained(t)?;
            Ok(())
        }),
        ("entry", |c, n, t| {
            c.entry = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("router", |c, n, t| {
            c.router = n.arg(t, 0)?.one::<Router>(t, NodeExt::span(n))?;
            Ok(())
        }),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `speculation { prefetch ..; prerender .. }` block. Its presence enables
/// the navigation hints.
impl Section for SpeculationConfig {
    const RULES: Block<Self> = Block(&[
        ("prefetch", |c, n, t| {
            c.prefetch = n.arg(t, 0)?.one::<Eagerness>(t, NodeExt::span(n))?;
            Ok(())
        }),
        ("prerender", |c, n, t| {
            c.prerender = n.arg(t, 0)?.one::<Eagerness>(t, NodeExt::span(n))?;
            Ok(())
        }),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `typst { .. }` section: typst engine knobs.
impl Section for TypstConfig {
    const RULES: Block<Self> = Block(&[
        ("features", |c, n, t| {
            c.features = n.features(t)?;
            Ok(())
        }),
        ("inputs", |c, n, t| {
            c.inputs = n.pairs(t)?;
            Ok(())
        }),
        // Stored without its trailing slash: the store joins `/preview/..` onto
        // it, and a doubled slash is a 404 from some hosts and a redirect from
        // others.
        ("registry", |c, n, t| {
            c.registry = Some(n.url(t, 0)?.trim_end_matches('/').to_owned());
            Ok(())
        }),
    ]);
}

impl Section for CacheConfig {
    const RULES: Block<Self> = Block(&[
        ("dir", |c, n, t| {
            c.dir = n.string(t, 0)?.into();
            Ok(())
        }),
        ("incremental", |c, n, t| {
            c.incremental = n.boolean(t, 0)?;
            Ok(())
        }),
    ]);
}

/// Each list replaces (never appends) so a profile overriding `before` leaves
/// `after` inherited from the base: same whole-value replacement as every other
/// list field.
impl Section for HooksConfig {
    const RULES: Block<Self> = Block(&[
        ("before", |c, n, t| {
            c.before = n.words(t)?;
            Ok(())
        }),
        ("after", |c, n, t| {
            c.after = n.words(t)?;
            Ok(())
        }),
    ]);
}

/// The `announce { .. }` section: one block per destination backend.
impl Section for AnnounceConfig {
    const RULES: Block<Self> = Block(&[("standard", |c, n, t| {
        StandardConfig::optional(&mut c.standard, n, t)
    })]);
}

/// The `standard { .. }` block: presence enables the standard.site backend.
impl Section for StandardConfig {
    const RULES: Block<Self> = Block(&[
        ("handle", |c, n, t| {
            c.handle = n.string(t, 0)?;
            Ok(())
        }),
        ("did", |c, n, t| {
            c.did = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("pds", |c, n, t| {
            c.pds = n.url(t, 0)?;
            Ok(())
        }),
        ("discover", |c, n, t| {
            c.discover = n.boolean(t, 0)?;
            Ok(())
        }),
        ("icon", |c, n, t| {
            c.icon = Some(n.string(t, 0)?.into());
            Ok(())
        }),
        ("verify", |c, n, t| c.verify.fill(n, t)),
    ]);
}

/// The `verify { wellknown; links }` block: which build-time verification
/// artifacts to emit.
impl Section for VerifyConfig {
    const RULES: Block<Self> = Block(&[
        ("wellknown", |c, n, t| {
            c.wellknown = n.boolean(t, 0)?;
            Ok(())
        }),
        ("links", |c, n, t| {
            c.links = n.boolean(t, 0)?;
            Ok(())
        }),
    ]);
}

/// The `deploy { .. }` section: one block per destination backend.
impl Section for DeployConfig {
    const RULES: Block<Self> = Block(&[
        ("s3", |c, n, t| S3Config::optional(&mut c.s3, n, t)),
        ("ssh", |c, n, t| SshConfig::optional(&mut c.ssh, n, t)),
    ]);
}

/// The `s3 { .. }` block: presence enables the S3 backend.
impl Section for S3Config {
    const RULES: Block<Self> = Block(&[
        ("bucket", |c, n, t| {
            c.bucket = n.string(t, 0)?;
            Ok(())
        }),
        ("endpoint", |c, n, t| {
            c.endpoint = Some(n.url(t, 0)?);
            Ok(())
        }),
        ("region", |c, n, t| {
            c.region = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("prefix", |c, n, t| {
            c.prefix = n.string(t, 0)?;
            Ok(())
        }),
        ("delete", |c, n, t| {
            c.delete = n.boolean(t, 0)?;
            Ok(())
        }),
    ]);
}

/// The top-level `caching { .. }` block: presence turns `Cache-Control` on, and
/// both values default to the conventional policy.
impl Section for CacheControl {
    fn enable(&mut self) -> bool {
        self.enabled = true;
        // Filled here rather than in `Default`, so an untouched `S3Config`
        // carries no policy at all and the two states stay distinguishable.
        if self.immutable.is_empty() {
            self.immutable = Self::IMMUTABLE.to_owned();
        }
        if self.default.is_empty() {
            self.default = Self::DEFAULT.to_owned();
        }
        true
    }

    const RULES: Block<Self> = Block(&[
        ("immutable", |c, n, t| {
            c.immutable = n.string(t, 0)?;
            Ok(())
        }),
        ("default", |c, n, t| {
            c.default = n.string(t, 0)?;
            Ok(())
        }),
    ]);
}

/// The `ssh { .. }` block: presence enables the SSH backend.
impl Section for SshConfig {
    const RULES: Block<Self> = Block(&[
        ("host", |c, n, t| {
            c.host = n.string(t, 0)?;
            Ok(())
        }),
        ("path", |c, n, t| {
            c.path = n.string(t, 0)?;
            Ok(())
        }),
        ("port", |c, n, t| {
            c.port = n.port(t, 0)?;
            Ok(())
        }),
        ("user", |c, n, t| {
            c.user = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("key", |c, n, t| {
            c.key = Some(n.string(t, 0)?.into());
            Ok(())
        }),
        ("strict", |c, n, t| {
            c.strict = n.boolean(t, 0)?;
            Ok(())
        }),
        ("delete", |c, n, t| {
            c.delete = n.boolean(t, 0)?;
            Ok(())
        }),
    ]);
}

impl Section for ServeConfig {
    const RULES: Block<Self> = Block(&[
        ("port", |c, n, t| {
            c.port = n.port(t, 0)?;
            Ok(())
        }),
        ("bind", |c, n, t| {
            c.bind = n.string(t, 0)?;
            Ok(())
        }),
        ("open", |c, n, t| {
            c.open = n.boolean(t, 0)?;
            Ok(())
        }),
        ("watch", |c, n, t| {
            c.watch = n.boolean(t, 0)?;
            Ok(())
        }),
        ("include", |c, n, t| {
            c.include = n.words(t)?;
            Ok(())
        }),
        ("exclude", |c, n, t| {
            c.exclude = n.words(t)?;
            Ok(())
        }),
    ]);
}
