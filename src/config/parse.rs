//! The config schema: every KDL key paired with the handler that applies it.
//!
//! Each section carries its own table by implementing [`Section`] (a `{ .. }`
//! block) or [`Attributed`] (a `key=value` line), so a scope's valid keys, its
//! parsing, and its "unknown key" suggestions all read one list. The typed
//! primitives those handlers are written in terms of live in [`super::node`] and
//! [`super::value`].

use kdl::{KdlDocument, KdlNode};

use crate::config::Named;
use crate::config::dispatch::Kind::{
    Block as Nested, Choice, Flag, Items, Line, Lines, Number, Numbers, Overlay, Path, Size, Table,
    Template, Text, Texts, Url,
};
use crate::config::dispatch::{Attributed, Attrs, Block, Section};
use crate::config::node::NodeExt;
use crate::config::permalink::Permalink;
use crate::config::value::ValueExt;
// The frontmatter key table is the single source of truth for what a built-in
// key holds, so a schema declaring one is checked against it rather than
// against a second list kept here.
use crate::config::{
    AnnounceConfig, AssetConfig, BudgetConfig, CacheConfig, CacheControl, CardsConfig,
    CollectionConfig, Config, ContentConfig, CspConfig, DeployConfig, DisplayMode, DraftConfig,
    Eagerness, FeedConfig, FeedKind, FeedNames, FieldSchema, FieldType, GenerateConfig,
    HooksConfig, HtmlConfig, IconConfig, IconPurpose, ImagesConfig, JpegConfig, LanguageConfig,
    LinkConfig, Linked, LintConfig, LlmsConfig, ManifestConfig, NavigationConfig, OptimizeConfig,
    PaginateConfig, Paths, PdfBundle, PdfConfig, PdfPages, PngConfig, PngStrip, Prefetch,
    ResponsiveConfig, RobotsConfig, Router, S3Config, SearchConfig, SearchField, SearchFormat,
    SecurityConfig, ServeConfig, SortKey, SpaConfig, SpeculationConfig, SshConfig,
    StandaloneConfig, StandardConfig, TaxonomyConfig, TypstConfig, UrlStyle, VerifyConfig,
};
use crate::content::Frontmatter;
use crate::error::{ConfigError, ConfigErrorKind, Result};
use crate::ui::markup;

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        // keep the raw text: profile overlay reports errors against it, its nodes carry spans into it
        let mut cfg = Self {
            source: text.to_owned(),
            ..Self::default()
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
            "paths",
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
            "Checks run over the built pages. Its presence turns them on.",
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
            "The `Cache-Control` policy uploaded files are given.",
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
            "hooks",
            Nested(HooksConfig::rows),
            "External commands run before and after the build.",
            |c, n, t| c.hooks.fill(n, t),
        ),
        (
            "announce",
            Nested(AnnounceConfig::rows),
            "Where to announce the site's metadata.",
            |c, n, t| c.announce.fill(n, t),
        ),
        (
            "deploy",
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

/// The `paths { .. }` section: directory layout knobs.
impl Section for Paths {
    const RULES: Block<Self> = Block(&[
        (
            "content",
            Path,
            "The content tree of `.typ` pages.",
            |c, n, t| {
                c.content = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "dist",
            Path,
            "Where the built site is written.",
            |c, n, t| {
                c.dist = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "assets",
            Path,
            "Assets that go through the pipeline: CSS, JS, images.",
            |c, n, t| {
                c.assets = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "static",
            Path,
            "Files copied to the output verbatim, untouched by the pipeline.",
            |c, n, t| {
                c.r#static = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "templates",
            Path,
            "Where layouts and partials are imported from.",
            |c, n, t| {
                c.templates = n.string(t, 0)?.into();
                Ok(())
            },
        ),
    ]);
}

/// The `content { .. }` section: what the content tree holds and how it is
/// read. The directory it lives in is `paths { content }`.
impl Section for ContentConfig {
    const RULES: Block<Self> = Block(&[
        (
            "index",
            Text,
            "The filename stem that publishes at its directory's own URL, without extension.",
            |c, n, t| {
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
            },
        ),
        (
            "future",
            Flag,
            "Build pages dated later than now.",
            |c, n, t| {
                c.future = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "draft",
            Nested(DraftConfig::rows),
            "Whether drafts are built, and where they land.",
            |c, n, t| c.draft.fill(n, t),
        ),
        (
            "collections",
            Items(CollectionConfig::rows),
            "One block per collection, each named by its id.",
            |c, n, t| {
                c.collections = n.unique(t, "collection", CollectionConfig::item)?;
                Ok(())
            },
        ),
        (
            "taxonomies",
            Lines(TaxonomyConfig::rows),
            "One line per taxonomy, each named by its id.",
            |c, n, t| {
                c.taxonomies = n.unique(t, "taxonomy", TaxonomyConfig::item)?;
                Ok(())
            },
        ),
    ]);
}

impl Section for DraftConfig {
    const RULES: Block<Self> = Block(&[
        ("build", Flag, "Build draft pages at all.", |c, n, t| {
            c.build = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "suffix",
            Text,
            "The filename marker that flags a draft, peeled off the stem: `post.draft.typ`.",
            |c, n, t| {
                c.suffix = n.string(t, 0)?;
                Ok(())
            },
        ),
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
        (
            "glob",
            Text,
            "Which content files belong to this collection.",
            |c, n, t| {
                c.glob = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "sort",
            Choice(SortKey::names),
            "What the collection's members are ordered by.",
            |c, n, t| {
                c.sort = n.arg(t, 0)?.one::<SortKey>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        ("reverse", Flag, "Reverse that order.", |c, n, t| {
            c.reverse = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "permalink",
            Template,
            "The URL pattern its pages publish at, e.g. `/{slug}/`.",
            |c, n, t| {
                let raw = n.string(t, 0)?;
                // validate here so a template typo is a spanned config error,
                // not a silent fallback to convention at page load
                Permalink::parse(&raw)
                    .map_err(|e| ConfigError::at(t, e.into(), NodeExt::span(n)))?;
                c.permalink = Some(raw);
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The layout its pages render through.",
            |c, n, t| {
                c.template = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "paginate",
            Nested(PaginateConfig::rows),
            "Generate an index over the collection. Its presence turns the index on.",
            |c, n, t| c.paginate.fill(n, t),
        ),
        (
            "feed",
            Flag,
            "Also write a feed of this collection's members, beside its index.",
            |c, n, t| {
                c.feed = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "schema",
            Items(FieldSchema::rows),
            "What every member's frontmatter must declare, one line per field. A `dict` field takes a block of its own fields.",
            |c, n, t| {
                c.schema = n.unique(t, "schema field", FieldSchema::item)?;
                Ok(())
            },
        ),
    ]);
}

impl FieldSchema {
    /// One `title "str" optional=#true` line: the node name is the frontmatter
    /// key it constrains, and an optional leading positional its type, so both
    /// the bare `title` (present, any shape) and the terse `tags "list"` read.
    ///
    /// A `{ .. }` block declares the fields of the dictionary the type ends in,
    /// through however many `list<..>` wrap it, and recurses through this same
    /// reader: `authors "list<dict>" { name "str" }`.
    fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let key = node.name().value().to_owned();
        let span = NodeExt::span(node);
        let mut field = Self::default();
        if let Some(value) = node.get(0) {
            field.ty = value.ty(text, span)?;
        }
        field.read(node, text)?;
        if node.children().is_some() {
            let fields = node.unique(text, "schema field", Self::item)?;
            let declared = field.ty.article();
            let Some(dict) = field.ty.fields_mut() else {
                return Err(ConfigError::at(
                    text,
                    ConfigErrorKind::FieldNotDict { key, declared },
                    span,
                )
                .into());
            };
            *dict = fields;
        }
        // A built-in key's type is fixed by the frontmatter reader, and that
        // reader runs first: it would reject the value before the schema ever
        // saw it. A contradiction is therefore unsatisfiable, and fails at the
        // line that wrote it rather than on every page of the collection.
        if let Some(builtin) = Frontmatter::builtin(&key)
            && field.ty != FieldType::Any
            && field.ty != builtin
        {
            return Err(ConfigError::at(
                text,
                ConfigErrorKind::FieldConflict {
                    key,
                    declared: field.ty.article(),
                    builtin: builtin.article(),
                },
                span,
            )
            .into());
        }
        Ok((key, field))
    }
}

/// One schema field: the shape a frontmatter value must have, and whether the
/// page may leave it out.
impl Attributed for FieldSchema {
    /// The type, written as the leading positional.
    const LEADING: usize = 1;

    /// The one attribute scope with a block of its own: `item` reads it as the
    /// fields of the dictionary the type ends in.
    const NESTS: bool = true;

    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "type",
            Choice(FieldType::names),
            "The shape the value must have, also writable as the leading positional: `title \"str\"`. A list names what it holds: `list<int>`, `list<dict>`.",
            |c, v, t, s| {
                c.ty = v.ty(t, s)?;
                Ok(())
            },
        ),
        (
            "optional",
            Flag,
            "Let the field be absent. Declaring a field otherwise requires it.",
            |c, v, t, s| {
                c.optional = v.boolean(t, s)?;
                Ok(())
            },
        ),
    ]);
}

/// The `paginate { .. }` block inside a collection: its presence generates the
/// index, and every key tunes it.
impl Section for PaginateConfig {
    const RULES: Block<Self> = Block(&[
        (
            "size",
            Number,
            "Pages per index page. Omitted, the index is one page.",
            |c, n, t| {
                let n_ = n.arg(t, 0)?.integer(t, NodeExt::span(n))?;
                if n_ < 1 {
                    return Err(ConfigError::paginate_too_small(t, n_, NodeExt::span(n)).into());
                }
                // `n_` is proved positive above; a page size wider than `usize`
                // (only reachable on a 32-bit target) still means "one page".
                c.size = Some(usize::try_from(n_).unwrap_or(usize::MAX));
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The layout the index renders through.",
            |c, n, t| {
                c.template = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "mount",
            Text,
            "Where the index publishes, if not at the collection's own path.",
            |c, n, t| {
                c.mount = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "prefix",
            Text,
            "The path segment before a page number, as in `/posts/page/2/`.",
            |c, n, t| {
                c.prefix = n.string(t, 0)?;
                Ok(())
            },
        ),
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
        (
            "key",
            Text,
            "The frontmatter field its terms are read from. Defaults to the taxonomy's own id.",
            |c, v, t, s| {
                c.key = v.as_str(t, s)?;
                Ok(())
            },
        ),
        (
            "listing",
            Flag,
            "Generate a page per term, and an index of the terms.",
            |c, v, t, s| {
                c.listing = v.boolean(t, s)?;
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The layout those listings render through.",
            |c, v, t, s| {
                c.template = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
        (
            "paginate",
            Number,
            "Pages per term listing.",
            |c, v, t, s| {
                let n = v.integer(t, s)?;
                if n < 1 {
                    return Err(ConfigError::paginate_too_small(t, n, s).into());
                }
                // `n` is proved positive above; a page size wider than `usize`
                // (only reachable on a 32-bit target) still means "one page".
                c.paginate = Some(usize::try_from(n).unwrap_or(usize::MAX));
                Ok(())
            },
        ),
        (
            "prefix",
            Text,
            "The path segment before a term page's number, as in `/tags/rust/page/2/`.",
            |c, v, t, s| {
                c.prefix = v.as_str(t, s)?;
                Ok(())
            },
        ),
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
        (
            "name",
            Text,
            "The language's name in its own language, for a switcher.",
            |c, n, t| {
                c.name = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "dir",
            Text,
            "Writing direction, `ltr` or `rtl`.",
            |c, n, t| {
                c.dir = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "site",
            Text,
            "The site name in this language.",
            |c, n, t| {
                c.site = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "author",
            Text,
            "The default author in this language.",
            |c, n, t| {
                c.author = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "description",
            Text,
            "What the site is, in this language.",
            |c, n, t| {
                c.description = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "strings",
            Table,
            "This language's UI string table, one `key value` line per entry.",
            |c, n, t| {
                c.strings = n.table(t)?;
                Ok(())
            },
        ),
    ]);
}

/// The `assets { .. }` section: the pipeline applied to `paths { assets }`.
impl Section for AssetConfig {
    const RULES: Block<Self> = Block(&[
        ("minify", Flag, "Minify CSS and JavaScript.", |c, n, t| {
            c.minify = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "bundle",
            Flag,
            "Bundle JavaScript modules into one file per entry point.",
            |c, n, t| {
                c.bundle = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "fingerprint",
            Flag,
            "Put a content hash in each asset's filename, so it can be cached forever.",
            |c, n, t| {
                c.fingerprint = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "tsconfig",
            Path,
            "The `tsconfig.json` TypeScript and JSX are transformed against. Unset, one is discovered per script.",
            |c, n, t| {
                c.tsconfig = Some(n.string(t, 0)?.into());
                Ok(())
            },
        ),
        (
            "images",
            Nested(ImagesConfig::rows),
            "Image markup and build-time processing.",
            |c, n, t| c.images.fill(n, t),
        ),
    ]);
}

/// The `images { .. }` section: markup annotations and build-time processing.
impl Section for ImagesConfig {
    const RULES: Block<Self> = Block(&[
        (
            "lazy",
            Flag,
            "Mark images `loading=\"lazy\"`.",
            |c, n, t| {
                c.lazy = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "extract",
            Flag,
            "Write images typst embedded in the page out as their own files.",
            |c, n, t| {
                c.extract = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "optimize",
            Nested(OptimizeConfig::rows),
            "Per-format lossless recompression.",
            |c, n, t| c.optimize.fill(n, t),
        ),
        (
            "responsive",
            Nested(ResponsiveConfig::rows),
            "Generate width variants and a `srcset`. Its presence turns them on.",
            |c, n, t| c.responsive.fill(n, t),
        ),
    ]);
}

/// The `responsive { widths .. ; quality N }` block. Its presence enables
/// width-variant generation; widths and quality keep their defaults unless
/// named.
impl Section for ResponsiveConfig {
    const RULES: Block<Self> = Block(&[
        (
            "widths",
            Numbers,
            "The pixel widths to emit a variant at.",
            |c, n, t| {
                c.widths = n.widths(t)?;
                Ok(())
            },
        ),
        (
            "quality",
            Number,
            "Encoder quality for the generated variants, 1 to 100.",
            |c, n, t| {
                c.quality = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, 100)?;
                Ok(())
            },
        ),
        (
            "sizes",
            Text,
            "The `sizes` attribute put on every responsive image.",
            |c, n, t| {
                c.sizes = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
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
        (
            "png",
            Line(PngConfig::rows),
            "Optimize PNGs. Its presence turns them on; the attributes tune it.",
            |c, n, t| c.png.get_or_insert_default().read(n, t),
        ),
        (
            "jpeg",
            Line(JpegConfig::rows),
            "Optimize JPEGs. Its presence turns them on; the attributes tune it.",
            |c, n, t| c.jpeg.get_or_insert_default().read(n, t),
        ),
    ]);
}

impl Attributed for PngConfig {
    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "level",
            Number,
            "Compression effort, 0 to 6. Higher is slower and smaller.",
            |c, v, t, s| {
                c.level = v.bounded(t, s, 0, 6)?;
                Ok(())
            },
        ),
        (
            "strip",
            Choice(PngStrip::names),
            "Which ancillary chunks to discard.",
            |c, v, t, s| {
                c.strip = v.one::<PngStrip>(t, s)?;
                Ok(())
            },
        ),
    ]);
}

impl Attributed for JpegConfig {
    const ATTRS: Attrs<Self> = Attrs(&[(
        "quality",
        Number,
        "Encoder quality, 1 to 100.",
        |c, v, t, s| {
            c.quality = v.bounded(t, s, 1, 100)?;
            Ok(())
        },
    )]);
}

/// The `html { .. }` section: post-processing of typst's HTML output.
impl Section for HtmlConfig {
    const RULES: Block<Self> = Block(&[
        ("pretty", Flag, "Indent the emitted HTML.", |c, n, t| {
            c.pretty = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "embed",
            Flag,
            "Inline processed assets into the page as `data:` URIs.",
            |c, n, t| {
                c.embed = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "meta",
            Flag,
            "Emit the `<meta>` description, Open Graph and Twitter tags.",
            |c, n, t| {
                c.meta = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "anchors",
            Flag,
            "Give every heading an `id` and a self link.",
            |c, n, t| {
                c.anchors = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "jsonld",
            Flag,
            "Emit JSON-LD structured data for each page.",
            |c, n, t| {
                c.jsonld = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "spans",
            Flag,
            "Stamp each element with the source span it came from, so `serve` can open it.",
            |c, n, t| {
                c.spans = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        // A list of element names, tried in order. Each is checked here, where
        // the span points at the word the author wrote: an unwritable name would
        // otherwise fail silently at render, as an element the page never has.
        (
            "footnotes",
            Texts,
            "The elements a page's footnotes belong inside, most specific first.",
            |c, n, t| {
                let span = NodeExt::span(n);
                let names = n.words(t)?;
                for name in &names {
                    // The DOM's own judgement of what can be an element, rather than
                    // a second opinion here. Checked at the span the author wrote,
                    // because a name no element can carry would otherwise fail
                    // silently at render, as a container the page simply never has.
                    typst_html::HtmlTag::intern(name).map_err(|why| {
                        ConfigError::unknown_value(
                            t,
                            name,
                            markup!("name an element your layout emits, like `article`: {}", why),
                            span,
                        )
                    })?;
                }
                c.footnotes = names.into();
                Ok(())
            },
        ),
        (
            "highlight",
            Table,
            "Rewrite syntax-highlight colours to classes. Bare, it uses hex classes; a block names the scopes.",
            |c, n, t| {
                c.highlight.enabled = true;
                // A bare `highlight` rewrites every colour to its hex class; a block
                // names the scopes the theme paints, so the classes read as meaning
                // rather than as colours.
                if n.children().is_some() {
                    c.highlight.scopes = n.pairs(t)?;
                }
                Ok(())
            },
        ),
    ]);
}

/// The `links { .. }` section: URL shape and link checking.
impl Section for LinkConfig {
    const RULES: Block<Self> = Block(&[
        (
            "style",
            Choice(UrlStyle::names),
            "Whether URLs are directories (`clean`) or `.html` files (`flat`).",
            |c, n, t| {
                c.style = n.arg(t, 0)?.one::<UrlStyle>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "strict",
            Flag,
            "Fail the build on a broken internal link instead of warning.",
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "external",
            Flag,
            "Also check outbound `http(s)` links over the network.",
            |c, n, t| {
                c.external = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "backlinks",
            Flag,
            "Hand each page the pages whose content links to it, as `page.backlinks`.",
            |c, n, t| {
                c.backlinks = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "orphans",
            Choice(Linked::names),
            "Report the pages nothing links to, counting `any` page's links or only those an author wrote.",
            |c, n, t| {
                c.orphans = Some(n.arg(t, 0)?.one::<Linked>(t, NodeExt::span(n))?);
                Ok(())
            },
        ),
    ]);
}

/// The `lint { .. }` section: the rules run over each built page's DOM, and how
/// loud a finding is. The block's presence is what turns linting on.
impl Section for LintConfig {
    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }

    const RULES: Block<Self> = Block(&[
        (
            "strict",
            Flag,
            "Fail the build on a finding instead of warning.",
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "headings",
            Flag,
            "Report a heading that skips a level, e.g. `h2` straight to `h4`.",
            |c, n, t| {
                c.headings = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "alt",
            Flag,
            "Report an image with no `alt` attribute at all (an empty one marks it decorative).",
            |c, n, t| {
                c.alt = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "ids",
            Flag,
            "Report an `id` used more than once on a page.",
            |c, n, t| {
                c.ids = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "aria",
            Flag,
            "Report an unknown ARIA role or attribute, and one referring to an id that is not there.",
            |c, n, t| {
                c.aria = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "budget",
            Nested(BudgetConfig::rows),
            "How many bytes one page may ship. Exceeding a budget always fails the build.",
            |c, n, t| c.budget.fill(n, t),
        ),
    ]);
}

/// The `lint { budget { .. } }` section: per-page weight ceilings.
impl Section for BudgetConfig {
    const RULES: Block<Self> = Block(&[
        (
            "html",
            Size,
            "The page's own markup, as written to the output directory.",
            |c, n, t| {
                c.html = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
        (
            "js",
            Size,
            "Every script the page loads, plus its inline `<script>` bodies.",
            |c, n, t| {
                c.js = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
        (
            "css",
            Size,
            "Every stylesheet it loads, plus its inline `<style>` bodies.",
            |c, n, t| {
                c.css = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
        (
            "images",
            Size,
            "Every image it references, responsive alternatives excluded.",
            |c, n, t| {
                c.images = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
        (
            "total",
            Size,
            "All of the above at once: the page's whole transfer weight.",
            |c, n, t| {
                c.total = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
    ]);
}

/// The `security { .. }` section: what the pages tell a browser to trust.
impl Section for SecurityConfig {
    const RULES: Block<Self> = Block(&[
        (
            "sri",
            Flag,
            "Stamp `integrity` onto every emitted script and stylesheet. Needs `assets { fingerprint }`.",
            |c, n, t| {
                c.sri = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "csp",
            Nested(CspConfig::rows),
            "The content security policy written into `_headers`. Its presence turns it on.",
            |c, n, t| c.csp.fill(n, t),
        ),
    ]);
}

/// The `security { csp { .. } }` section: one key per directive, each taking a
/// CSP source list verbatim.
impl Section for CspConfig {
    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }

    const RULES: Block<Self> = Block(&[
        (
            "enforce",
            Flag,
            "Enforce the policy. Off reports violations without blocking anything.",
            |c, n, t| {
                c.enforce = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "hashes",
            Flag,
            "Add the digest of every inline script and style the build produced. Turns `html { pretty }` off, since a digest has to cover the bytes as served.",
            |c, n, t| {
                c.hashes = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "default",
            Text,
            "`default-src`: what every unstated fetch directive falls back to.",
            |c, n, t| {
                c.default = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        ("script", Text, "`script-src`.", |c, n, t| {
            c.script = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("style", Text, "`style-src`.", |c, n, t| {
            c.style = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("img", Text, "`img-src`.", |c, n, t| {
            c.img = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("font", Text, "`font-src`.", |c, n, t| {
            c.font = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("connect", Text, "`connect-src`.", |c, n, t| {
            c.connect = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("frame", Text, "`frame-src`.", |c, n, t| {
            c.frame = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("object", Text, "`object-src`.", |c, n, t| {
            c.object = Some(n.string(t, 0)?);
            Ok(())
        }),
        (
            "base",
            Text,
            "`base-uri`: what a `<base>` may repoint relative URLs at.",
            |c, n, t| {
                c.base = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "form",
            Text,
            "`form-action`: where a form may submit.",
            |c, n, t| {
                c.form = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "report",
            Url,
            "`report-uri`: where a violation report is posted.",
            |c, n, t| {
                c.report = Some(n.url(t, 0)?);
                Ok(())
            },
        ),
    ]);
}

/// The `generate { .. }` section: the files a build emits beside the pages.
/// Each child is opt-in, either a flag or a block whose presence turns it on.
impl Section for GenerateConfig {
    const RULES: Block<Self> = Block(&[
        ("sitemap", Flag, "Write `sitemap.xml`.", |c, n, t| {
            c.sitemap = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "redirects",
            Flag,
            "Write a `_redirects` file from each page's declared aliases.",
            |c, n, t| {
                c.redirects = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "headers",
            Flag,
            "Write a `_headers` file from the caching policy.",
            |c, n, t| {
                c.headers = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "robots",
            Nested(RobotsConfig::rows),
            "Write `robots.txt`. Its presence turns it on.",
            |c, n, t| c.robots.fill(n, t),
        ),
        (
            "llms",
            Nested(LlmsConfig::rows),
            "Write `llms.txt`. Its presence turns it on.",
            |c, n, t| c.llms.fill(n, t),
        ),
        (
            "manifest",
            Nested(ManifestConfig::rows),
            "Write `manifest.webmanifest`. Its presence turns it on.",
            |c, n, t| c.manifest.fill(n, t),
        ),
        (
            "feed",
            Nested(FeedConfig::rows),
            "Write syndication feeds.",
            |c, n, t| c.feed.fill(n, t),
        ),
        (
            "search",
            Nested(SearchConfig::rows),
            "Write a client-side search index.",
            |c, n, t| c.search.fill(n, t),
        ),
        (
            "cards",
            Nested(CardsConfig::rows),
            "Draw a social card per page. Its presence turns it on.",
            |c, n, t| c.cards.fill(n, t),
        ),
        (
            "pdf",
            Nested(PdfConfig::rows),
            "Typeset PDFs beside the pages.",
            |c, n, t| c.pdf.fill(n, t),
        ),
    ]);
}

impl Section for RobotsConfig {
    const RULES: Block<Self> = Block(&[(
        "disallow",
        Texts,
        "Paths to disallow, one word each.",
        |c, n, t| {
            c.disallow = n.words(t)?;
            Ok(())
        },
    )]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

impl Section for LlmsConfig {
    const RULES: Block<Self> = Block(&[(
        "summary",
        Text,
        "A one-line description of the site, put at the top of the file.",
        |c, n, t| {
            c.summary = Some(n.string(t, 0)?);
            Ok(())
        },
    )]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `manifest { .. }` block. Its presence enables the manifest; every key is
/// something only the author knows, since what the build knows it fills in
/// itself.
impl Section for ManifestConfig {
    const RULES: Block<Self> = Block(&[
        (
            "name",
            Text,
            "The installed app's name. Defaults to the site title.",
            |c, n, t| {
                c.name = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "short",
            Text,
            "The name a launcher shows when the full one does not fit.",
            |c, n, t| {
                c.short = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "description",
            Text,
            "One line about the app, shown by an install prompt.",
            |c, n, t| {
                c.description = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "display",
            Choice(DisplayMode::names),
            "How the installed app is presented.",
            |c, n, t| {
                c.display = n.arg(t, 0)?.one::<DisplayMode>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "theme",
            Text,
            "CSS colour of the browser UI around the app, and of every page's `theme-color`.",
            |c, n, t| {
                c.theme = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "background",
            Text,
            "CSS colour painted before the first page has rendered.",
            |c, n, t| {
                c.background = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "start",
            Text,
            "Where launching the installed app lands, per language. Defaults to the language's root.",
            |c, n, t| {
                c.start = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "scope",
            Text,
            "The URLs the installed app covers, per language. Defaults to the language's root.",
            |c, n, t| {
                c.scope = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "icons",
            Lines(IconConfig::rows),
            "One line per icon, each named by the path it is served from.",
            |c, n, t| {
                c.icons = n
                    .unique(t, "icon", IconConfig::item)?
                    .into_iter()
                    .map(|(_, icon)| icon)
                    .collect();
                Ok(())
            },
        ),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

impl IconConfig {
    /// One `"/icon-512.png" size=512` line: the node name is the path the image
    /// is served from, which is also what makes two icons the same icon.
    fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let src = node.name().value().to_owned();
        let mut icon = Self::from(src.clone());
        icon.read(node, text)?;
        Ok((src, icon))
    }
}

impl Attributed for IconConfig {
    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "size",
            Number,
            "The square edge in pixels. Absent means the image scales to any size.",
            |c, v, t, s| {
                c.size = Some(v.bounded(t, s, 1, 4096)?);
                Ok(())
            },
        ),
        (
            "purpose",
            Choice(IconPurpose::names),
            "What a launcher may do with the image.",
            |c, v, t, s| {
                c.purpose = v.one::<IconPurpose>(t, s)?;
                Ok(())
            },
        ),
    ]);
}

impl Section for FeedConfig {
    const RULES: Block<Self> = Block(&[
        (
            "formats",
            Choice(FeedKind::names),
            "Which feed formats to write, one word each.",
            |c, n, t| {
                c.formats = n.mapped::<FeedKind>(t)?;
                Ok(())
            },
        ),
        (
            "limit",
            Number,
            "How many of the newest pages a feed carries.",
            |c, n, t| {
                c.limit = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "terms",
            Flag,
            "Also write a feed per taxonomy term.",
            |c, n, t| {
                c.terms = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "names",
            Nested(FeedNames::rows),
            "What each format's file is called, if not the conventional name.",
            |c, n, t| c.names.fill(n, t),
        ),
    ]);
}

/// The `feed { names { .. } }` section: one key per format, each naming the
/// file that format is written to and advertised under. A site arriving from a
/// generator that named them differently keeps its subscribers by stating the
/// old names here.
impl Section for FeedNames {
    const RULES: Block<Self> = Block(&[
        (
            "rss",
            Text,
            "The RSS file's name, e.g. `index.xml`. Defaults to `rss.xml`.",
            |c, n, t| {
                c.rss = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "atom",
            Text,
            "The Atom file's name. Defaults to `atom.xml`.",
            |c, n, t| {
                c.atom = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "json",
            Text,
            "The JSON Feed file's name. Defaults to `feed.json`.",
            |c, n, t| {
                c.json = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
    ]);
}

impl Section for SearchConfig {
    const RULES: Block<Self> = Block(&[
        (
            "formats",
            Choice(SearchFormat::names),
            "Which index formats to write, one word each.",
            |c, n, t| {
                c.formats = n.mapped::<SearchFormat>(t)?;
                Ok(())
            },
        ),
        (
            "fields",
            Choice(SearchField::names),
            "Which parts of a page go into the index, one word each.",
            |c, n, t| {
                c.fields = n.mapped::<SearchField>(t)?;
                Ok(())
            },
        ),
        (
            "region",
            Text,
            "The element whose contents are indexed, by tag name. \
             A page without one is indexed whole.",
            |c, n, t| {
                c.region = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "ignore",
            Texts,
            "Elements to leave out of the indexed region, by tag name, one word each.",
            |c, n, t| {
                c.ignore = n.words(t)?;
                Ok(())
            },
        ),
        (
            "stopwords",
            Texts,
            "Words to leave out of the index, one word each.",
            |c, n, t| {
                c.stopwords = n.words(t)?;
                Ok(())
            },
        ),
        (
            "minimum",
            Number,
            "The shortest word the index keeps.",
            |c, n, t| {
                c.min_length = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "ui",
            Flag,
            "Ship the bundled search box as well as the index.",
            |c, n, t| {
                c.ui = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}

/// The `cards { template ..; width ..; height .. }` block. Its presence enables
/// social card rendering.
impl Section for CardsConfig {
    const RULES: Block<Self> = Block(&[
        (
            "template",
            Text,
            "The typst template each card is drawn with.",
            |c, n, t| {
                c.template = n.string(t, 0)?;
                Ok(())
            },
        ),
        ("width", Number, "Card width in pixels.", |c, n, t| {
            c.width = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, Self::MAX)?;
            Ok(())
        }),
        ("height", Number, "Card height in pixels.", |c, n, t| {
            c.height = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, Self::MAX)?;
            Ok(())
        }),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `pdf { .. }` section: what the typesetter writes on paper. Each child
/// block's presence enables that artifact.
impl Section for PdfConfig {
    const RULES: Block<Self> = Block(&[
        (
            "pages",
            Nested(PdfPages::rows),
            "A PDF per page, beside its HTML. Its presence turns it on.",
            |c, n, t| c.pages.fill(n, t),
        ),
        (
            "bundle",
            Nested(PdfBundle::rows),
            "Many pages as one document. Needs a target named below.",
            |c, n, t| c.bundle.fill(n, t),
        ),
    ]);
}

/// The `pages { template .. }` block. Its presence enables the per-page PDF.
impl Section for PdfPages {
    const RULES: Block<Self> = Block(&[(
        "template",
        Text,
        "The typst template each page is typeset with.",
        |c, n, t| {
            c.template = n.string(t, 0)?;
            Ok(())
        },
    )]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `bundle { template ..; collections ..; site .. }` block: many pages as
/// one document. Unlike its neighbours, presence alone enables nothing: a
/// bundle needs a target to bind.
impl Section for PdfBundle {
    const RULES: Block<Self> = Block(&[
        (
            "template",
            Text,
            "The typst template the bundle is typeset with.",
            |c, n, t| {
                c.template = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "collections",
            Texts,
            "Which collections the bundle gathers, one word each.",
            |c, n, t| {
                c.collections = n.words(t)?;
                Ok(())
            },
        ),
        (
            "site",
            Flag,
            "Bundle the whole site rather than named collections.",
            |c, n, t| {
                c.site = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);

    /// Presence is recorded but enables nothing: a bundle binds what the block
    /// names, and naming nothing is asking for nothing.
    fn enable(&mut self) -> bool {
        self.present = true;
        true
    }
}

/// The `navigation { .. }` section: how a visitor moves between the built
/// pages. Each child block's presence enables that strategy.
impl Section for NavigationConfig {
    const RULES: Block<Self> = Block(&[
        (
            "spa",
            Nested(SpaConfig::rows),
            "Client-side navigation between pages. Its presence turns it on.",
            |c, n, t| c.spa.fill(n, t),
        ),
        (
            "standalone",
            Nested(StandaloneConfig::rows),
            "Export the whole site as one HTML file. Its presence turns it on.",
            |c, n, t| c.standalone.fill(n, t),
        ),
        (
            "speculation",
            Nested(SpeculationConfig::rows),
            "Browser prefetch and prerender hints. Its presence turns them on.",
            |c, n, t| c.speculation.fill(n, t),
        ),
    ]);
}

/// The `spa { root ..; prefetch .. }` block. Its presence enables the
/// client-side navigation runtime.
impl Section for SpaConfig {
    const RULES: Block<Self> = Block(&[
        (
            "root",
            Text,
            "The element swapped out on navigation.",
            |c, n, t| {
                c.root = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "prefetch",
            Choice(Prefetch::names),
            "When to fetch a page ahead of the click.",
            |c, n, t| {
                c.prefetch = n.arg(t, 0)?.one::<Prefetch>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
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
        (
            "file",
            Path,
            "The single file the site is written to.",
            |c, n, t| {
                c.file = n.contained(t)?;
                Ok(())
            },
        ),
        ("entry", Text, "The page that file opens on.", |c, n, t| {
            c.entry = Some(n.string(t, 0)?);
            Ok(())
        }),
        (
            "router",
            Choice(Router::names),
            "How it addresses pages once opened.",
            |c, n, t| {
                c.router = n.arg(t, 0)?.one::<Router>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
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
        (
            "prefetch",
            Choice(Eagerness::names),
            "How eagerly the browser fetches a linked page.",
            |c, n, t| {
                c.prefetch = n.arg(t, 0)?.one::<Eagerness>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "prerender",
            Choice(Eagerness::names),
            "How eagerly it renders one ahead of the click.",
            |c, n, t| {
                c.prerender = n.arg(t, 0)?.one::<Eagerness>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `typst { .. }` section: typst engine knobs.
impl Section for TypstConfig {
    const RULES: Block<Self> = Block(&[
        (
            "features",
            Texts,
            "Typst language features to enable, or `-name` to disable one. `html` cannot be removed.",
            |c, n, t| {
                c.features = n.features(t)?;
                Ok(())
            },
        ),
        (
            "inputs",
            Table,
            "Values passed to every compile as `sys.inputs`, one `key value` line per entry.",
            |c, n, t| {
                c.inputs = n.pairs(t)?;
                Ok(())
            },
        ),
        // Stored without its trailing slash: the store joins `/preview/..` onto
        // it, and a doubled slash is a 404 from some hosts and a redirect from
        // others.
        (
            "registry",
            Url,
            "Where typst packages are fetched from.",
            |c, n, t| {
                c.registry = Some(n.url(t, 0)?.trim_end_matches('/').to_owned());
                Ok(())
            },
        ),
    ]);
}

impl Section for CacheConfig {
    const RULES: Block<Self> = Block(&[
        (
            "dir",
            Path,
            "Where incremental build state is kept.",
            |c, n, t| {
                c.dir = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "incremental",
            Flag,
            "Reuse that state. Off, every build is a cold one.",
            |c, n, t| {
                c.incremental = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}

/// Each list replaces (never appends) so a profile overriding `before` leaves
/// `after` inherited from the base: same whole-value replacement as every other
/// list field.
impl Section for HooksConfig {
    const RULES: Block<Self> = Block(&[
        (
            "before",
            Texts,
            "Commands run before the asset pipeline, so what they generate is picked up.",
            |c, n, t| {
                c.before = n.words(t)?;
                Ok(())
            },
        ),
        (
            "after",
            Texts,
            "Commands run once the output directory is written.",
            |c, n, t| {
                c.after = n.words(t)?;
                Ok(())
            },
        ),
    ]);
}

/// The `announce { .. }` section: one block per destination backend.
impl Section for AnnounceConfig {
    const RULES: Block<Self> = Block(&[(
        "standard",
        Nested(StandardConfig::rows),
        "Announce to standard.site over atproto. Its presence turns it on.",
        |c, n, t| StandardConfig::optional(&mut c.standard, n, t),
    )]);
}

/// The `standard { .. }` block: presence enables the standard.site backend.
impl Section for StandardConfig {
    const RULES: Block<Self> = Block(&[
        (
            "handle",
            Text,
            "The atproto handle the site is announced under.",
            |c, n, t| {
                c.handle = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "did",
            Text,
            "That handle's DID, if it should not be resolved at build time.",
            |c, n, t| {
                c.did = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "pds",
            Url,
            "The personal data server the record is written to.",
            |c, n, t| {
                c.pds = n.url(t, 0)?;
                Ok(())
            },
        ),
        (
            "discover",
            Flag,
            "Show the publication on standard.site's discovery surfaces.",
            |c, n, t| {
                c.discover = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "icon",
            Path,
            "An icon published with the record.",
            |c, n, t| {
                c.icon = Some(n.string(t, 0)?.into());
                Ok(())
            },
        ),
        (
            "verify",
            Nested(VerifyConfig::rows),
            "Which handle-verification artifacts the build emits.",
            |c, n, t| c.verify.fill(n, t),
        ),
    ]);
}

/// The `verify { wellknown; links }` block: which build-time verification
/// artifacts to emit.
impl Section for VerifyConfig {
    const RULES: Block<Self> = Block(&[
        (
            "wellknown",
            Flag,
            "Write `/.well-known/site.standard.publication`, naming the publication record.",
            |c, n, t| {
                c.wellknown = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "links",
            Flag,
            "Add the verification links to the page head.",
            |c, n, t| {
                c.links = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}

/// The `deploy { .. }` section: one block per destination backend.
impl Section for DeployConfig {
    const RULES: Block<Self> = Block(&[
        (
            "s3",
            Nested(S3Config::rows),
            "Upload to S3 or an S3-compatible bucket. Its presence turns it on.",
            |c, n, t| S3Config::optional(&mut c.s3, n, t),
        ),
        (
            "ssh",
            Nested(SshConfig::rows),
            "Upload over SSH. Its presence turns it on.",
            |c, n, t| SshConfig::optional(&mut c.ssh, n, t),
        ),
    ]);
}

/// The `s3 { .. }` block: presence enables the S3 backend.
impl Section for S3Config {
    const RULES: Block<Self> = Block(&[
        ("bucket", Text, "The bucket uploaded into.", |c, n, t| {
            c.bucket = n.string(t, 0)?;
            Ok(())
        }),
        (
            "endpoint",
            Url,
            "The API endpoint, for an S3-compatible host such as R2.",
            |c, n, t| {
                c.endpoint = Some(n.url(t, 0)?);
                Ok(())
            },
        ),
        ("region", Text, "The bucket's region.", |c, n, t| {
            c.region = Some(n.string(t, 0)?);
            Ok(())
        }),
        (
            "prefix",
            Text,
            "A key prefix every uploaded object goes under.",
            |c, n, t| {
                c.prefix = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "delete",
            Flag,
            "Delete remote objects this build did not produce.",
            |c, n, t| {
                c.delete = n.boolean(t, 0)?;
                Ok(())
            },
        ),
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
            Self::IMMUTABLE.clone_into(&mut self.immutable);
        }
        if self.default.is_empty() {
            Self::DEFAULT.clone_into(&mut self.default);
        }
        true
    }

    const RULES: Block<Self> = Block(&[
        (
            "immutable",
            Text,
            "The `Cache-Control` value for fingerprinted assets, which can be cached forever.",
            |c, n, t| {
                c.immutable = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "default",
            Text,
            "The `Cache-Control` value for everything else.",
            |c, n, t| {
                c.default = n.string(t, 0)?;
                Ok(())
            },
        ),
    ]);
}

/// The `ssh { .. }` block: presence enables the SSH backend.
impl Section for SshConfig {
    const RULES: Block<Self> = Block(&[
        ("host", Text, "The host uploaded to.", |c, n, t| {
            c.host = n.string(t, 0)?;
            Ok(())
        }),
        (
            "path",
            Text,
            "The remote directory the site is written into.",
            |c, n, t| {
                c.path = n.string(t, 0)?;
                Ok(())
            },
        ),
        ("port", Number, "The SSH port.", |c, n, t| {
            c.port = n.port(t, 0)?;
            Ok(())
        }),
        ("user", Text, "The user to connect as.", |c, n, t| {
            c.user = Some(n.string(t, 0)?);
            Ok(())
        }),
        (
            "key",
            Path,
            "The private key to authenticate with. Prefer an ed25519 key.",
            |c, n, t| {
                c.key = Some(n.string(t, 0)?.into());
                Ok(())
            },
        ),
        (
            "strict",
            Flag,
            "Refuse to connect to a host whose key is not already known.",
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "delete",
            Flag,
            "Delete remote files this build did not produce.",
            |c, n, t| {
                c.delete = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}

impl Section for ServeConfig {
    const RULES: Block<Self> = Block(&[
        (
            "port",
            Number,
            "The port the dev server listens on.",
            |c, n, t| {
                c.port = n.port(t, 0)?;
                Ok(())
            },
        ),
        (
            "bind",
            Text,
            "The address it binds. Defaults to loopback; it has no authentication.",
            |c, n, t| {
                c.bind = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "open",
            Flag,
            "Open a browser when the server starts.",
            |c, n, t| {
                c.open = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "watch",
            Flag,
            "Watch the sources and rebuild. Off, it serves what is already built.",
            |c, n, t| {
                c.watch = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "include",
            Texts,
            "Extra paths to watch, one word each.",
            |c, n, t| {
                c.include = n.words(t)?;
                Ok(())
            },
        ),
        (
            "exclude",
            Texts,
            "Paths to leave unwatched, one word each.",
            |c, n, t| {
                c.exclude = n.words(t)?;
                Ok(())
            },
        ),
        // The program and its arguments, each its own word: the command is run
        // directly, never through a shell, so a whole command line in one
        // string would name a program that does not exist. Caught here, where
        // the span points at what the author wrote, rather than as a spawn
        // failure on the first alt-click.
        (
            "editor",
            Texts,
            "The command alt-clicking a preview element runs, program and arguments as separate words.",
            |c, n, t| {
                let span = NodeExt::span(n);
                let words = n.words(t)?;
                if let [only] = words.as_slice()
                    && only.split_whitespace().count() > 1
                {
                    return Err(ConfigError::unknown_value(
                    t,
                    only,
                    markup!(
                        "give the program and each argument as its own word, like `editor \"code\" \"--goto\" \"{{file}}:{{line}}:{{column}}\"`"
                    ),
                    span,
                )
                .into());
                }
                c.editor = words;
                Ok(())
            },
        ),
    ]);
}
