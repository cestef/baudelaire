use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::SourceSpan;

use crate::config::dispatch::{Attrs, Block, Keys};
use crate::config::value::ValueExt;
use crate::config::{
    AssetConfig, CacheConfig, CollectionConfig, Config, DraftConfig, FeedConfig, FeedKind,
    HooksConfig, HtmlConfig, ImagesConfig, JpegConfig, LinkConfig, LlmsConfig, OptimizeConfig,
    PngConfig, PngStrip, RobotsConfig, SearchConfig, SearchField, SearchFormat, ServeConfig,
    TaxonomyConfig,
};
use crate::content::Permalink;
use crate::error::{ConfigError, Result};

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        // Keep the raw text: profile overlay reports errors against it, since
        // the retained profile nodes carry spans into this very string.
        let mut cfg = Config {
            source: text.to_owned(),
            ..Config::default()
        };
        Self::RULES.apply(&mut cfg, doc.nodes(), text)?;
        Ok(cfg)
    }

    /// Apply a single config node over `self` — used to overlay profile nodes
    /// (see [`Config::with_profile`]).
    pub(crate) fn overlay(&mut self, text: &str, node: &KdlNode) -> Result<()> {
        Self::RULES.apply(self, std::slice::from_ref(node), text)
    }

    /// The top-level config schema: every key paired with the handler that
    /// applies it. This table is the *single source of truth* for what keys are
    /// valid — dispatch and "unknown key" suggestions both read it.
    const RULES: Block<Config> = Block(&[
        ("site", |c, n, t| { c.site = Some(n.string(t, 0)?); Ok(()) }),
        ("url", |c, n, t| { c.url = Some(n.string(t, 0)?); Ok(()) }),
        ("lang", |c, n, t| { c.lang = n.string(t, 0)?; Ok(()) }),
        ("author", |c, n, t| { c.author = Some(n.string(t, 0)?); Ok(()) }),
        ("paths", |c, n, t| n.paths(c, t)),
        ("clean", |c, n, t| { c.clean = n.boolean(t, 0)?; Ok(()) }),
        ("future", |c, n, t| { c.future = n.boolean(t, 0)?; Ok(()) }),
        ("draft", |c, n, t| { c.draft = n.draft(t)?; Ok(()) }),
        ("links", |c, n, t| { c.links = n.links(t)?; Ok(()) }),
        ("cache", |c, n, t| { c.cache = n.cache(t)?; Ok(()) }),
        ("typst", |c, n, t| n.typst(c, t)),
        ("output", |c, n, t| n.output(c, t)),
        ("collections", |c, n, t| { c.collections = n.collections(t)?; Ok(()) }),
        ("taxonomies", |c, n, t| { c.taxonomies = n.taxonomies(t)?; Ok(()) }),
        ("hooks", |c, n, t| { c.hooks = n.hooks(t)?; Ok(()) }),
        ("serve", |c, n, t| { c.serve = n.serve(t)?; Ok(()) }),
        ("profiles", |c, n, t| { c.profiles = n.profiles(t)?; Ok(()) }),
    ]);
}

/// Typed accessors over a [`KdlNode`], plus the parent-section builders that map
/// a `{ … }` block onto its config struct. Shared with [`super::dispatch`],
/// which needs [`NodeExt::span`] and [`NodeExt::block`].
pub(super) trait NodeExt {
    fn span(&self) -> SourceSpan;
    fn string(&self, text: &str, idx: usize) -> Result<String>;
    fn arg(&self, text: &str, idx: usize) -> Result<&KdlValue>;
    fn boolean(&self, text: &str, idx: usize) -> Result<bool>;
    fn int(&self, text: &str, idx: usize) -> Result<i64>;
    fn count(&self, text: &str, idx: usize) -> Result<usize>;
    fn port(&self, text: &str, idx: usize) -> Result<u16>;
    fn block(&self, text: &str) -> Result<&KdlDocument>;
    /// The node's `{ … }` children parsed as `(id, item)` pairs, erroring on a
    /// duplicate id — the single dedup rule for collections, taxonomies, and
    /// profiles, where a repeated id would otherwise silently lose one side.
    fn unique<T>(
        &self,
        text: &str,
        noun: &'static str,
        each: fn(&KdlNode, &str) -> Result<(String, T)>,
    ) -> Result<Vec<(String, T)>>;
    fn paths(&self, config: &mut Config, text: &str) -> Result<()>;
    fn typst(&self, config: &mut Config, text: &str) -> Result<()>;
    fn output(&self, config: &mut Config, text: &str) -> Result<()>;
    fn inputs(&self, text: &str) -> Result<Vec<(String, String)>>;
    fn features(&self, text: &str) -> Result<Vec<String>>;
    fn words(&self, text: &str) -> Result<Vec<String>>;
    fn mapped<T: Copy>(&self, text: &str, table: &[(&'static str, T)]) -> Result<Vec<T>>;
    fn collections(&self, text: &str) -> Result<Vec<(String, CollectionConfig)>>;
    fn taxonomies(&self, text: &str) -> Result<Vec<(String, TaxonomyConfig)>>;
    fn as_collection(&self, text: &str) -> Result<(String, CollectionConfig)>;
    fn as_taxonomy(&self, text: &str) -> Result<(String, TaxonomyConfig)>;
    fn html(&self, text: &str) -> Result<HtmlConfig>;
    fn images(&self, text: &str) -> Result<ImagesConfig>;
    fn optimize(&self, text: &str) -> Result<OptimizeConfig>;
    fn png(&self, text: &str) -> Result<PngConfig>;
    fn jpeg(&self, text: &str) -> Result<JpegConfig>;
    fn assets(&self, text: &str) -> Result<AssetConfig>;
    fn robots(&self, text: &str) -> Result<RobotsConfig>;
    fn llms(&self, text: &str) -> Result<LlmsConfig>;
    fn draft(&self, text: &str) -> Result<DraftConfig>;
    fn links(&self, text: &str) -> Result<LinkConfig>;
    fn feed(&self, text: &str) -> Result<FeedConfig>;
    fn search(&self, text: &str) -> Result<SearchConfig>;
    fn cache(&self, text: &str) -> Result<CacheConfig>;
    fn hooks(&self, text: &str) -> Result<HooksConfig>;
    fn serve(&self, text: &str) -> Result<ServeConfig>;
    fn profiles(&self, text: &str) -> Result<Vec<(String, KdlDocument)>>;
}

impl NodeExt for KdlNode {
    /// Bridge kdl's `miette::SourceSpan` (its own miette 7) to ours.
    fn span(&self) -> SourceSpan {
        let s = KdlNode::span(self);
        SourceSpan::new(s.offset().into(), s.len())
    }

    fn string(&self, text: &str, idx: usize) -> Result<String> {
        self.arg(text, idx)?.as_str(text, NodeExt::span(self))
    }

    fn arg(&self, text: &str, idx: usize) -> Result<&KdlValue> {
        match self.get(idx) {
            Some(value) => Ok(value),
            None => Err(ConfigError::missing_arg(text, self.name().value(), NodeExt::span(self)).into()),
        }
    }

    /// A flag node's boolean: a bare node (`clean`) enables, a present argument
    /// must be a KDL boolean (`clean #false`) — anything else is a type error,
    /// never a silent coercion.
    fn boolean(&self, text: &str, idx: usize) -> Result<bool> {
        match self.get(idx) {
            None => Ok(true),
            Some(value) => value.boolean(text, NodeExt::span(self)),
        }
    }

    fn int(&self, text: &str, idx: usize) -> Result<i64> {
        self.arg(text, idx)?.integer(text, NodeExt::span(self))
    }

    /// A non-negative integer argument (a count), erroring on negatives with
    /// the node's own name in the message.
    fn count(&self, text: &str, idx: usize) -> Result<usize> {
        let n = self.int(text, idx)?;
        usize::try_from(n).map_err(|_| {
            ConfigError::bad_value(
                text,
                format!("`{}` must not be negative, got {n}", self.name().value()),
                NodeExt::span(self),
            )
            .into()
        })
    }

    /// A TCP port argument, range-checked so `port 99999` errors instead of
    /// wrapping to a different port.
    fn port(&self, text: &str, idx: usize) -> Result<u16> {
        let n = self.int(text, idx)?;
        u16::try_from(n).map_err(|_| {
            ConfigError::bad_value(text, format!("port must be 0-65535, got {n}"), NodeExt::span(self))
                .into()
        })
    }

    fn block(&self, text: &str) -> Result<&KdlDocument> {
        self.children()
            .ok_or_else(|| ConfigError::missing_children(text, NodeExt::span(self)).into())
    }

    fn unique<T>(
        &self,
        text: &str,
        noun: &'static str,
        each: fn(&KdlNode, &str) -> Result<(String, T)>,
    ) -> Result<Vec<(String, T)>> {
        let children = self.block(text)?;
        let mut out: Vec<(String, T)> = Vec::new();
        for node in children.nodes() {
            let (id, item) = each(node, text)?;
            if out.iter().any(|(seen, _)| *seen == id) {
                return Err(ConfigError::bad_value(
                    text,
                    format!("duplicate {noun} `{id}`"),
                    NodeExt::span(node),
                )
                .into());
            }
            out.push((id, item));
        }
        Ok(out)
    }

    fn inputs(&self, text: &str) -> Result<Vec<(String, String)>> {
        let children = self.block(text)?;
        children
            .nodes()
            .iter()
            .map(|child| Ok((child.name().value().to_owned(), child.string(text, 0)?)))
            .collect()
    }

    fn features(&self, text: &str) -> Result<Vec<String>> {
        let positional: Vec<_> = self
            .entries()
            .iter()
            .filter(|e| e.name().is_none())
            .collect();
        if positional.is_empty() {
            return Err(ConfigError::missing_arg(text, "features", NodeExt::span(self)).into());
        }
        positional
            .iter()
            .map(|entry| {
                let raw = entry.value().as_str(text, NodeExt::span(self))?;
                // A `-` prefix looks like it disables a feature, but nothing
                // ever subtracts — erroring beats silently *enabling* it.
                if let Some(name) = raw.strip_prefix('-') {
                    return Err(ConfigError::bad_value(
                        text,
                        format!(
                            "removing feature `{name}` is not supported; list the features you want enabled"
                        ),
                        NodeExt::span(self),
                    )
                    .into());
                }
                Ok(raw.strip_prefix('+').unwrap_or(&raw).to_owned())
            })
            .collect()
    }

    /// The node's positional string args, verbatim (e.g. `stopwords "a" "the"`).
    fn words(&self, text: &str) -> Result<Vec<String>> {
        let span = NodeExt::span(self);
        self.entries()
            .iter()
            .filter(|e| e.name().is_none())
            .map(|e| e.value().as_str(text, span))
            .collect()
    }

    /// Map the node's positional string args through a `(name, value)` table,
    /// erroring on the first unknown name with a nearest-match hint. The single
    /// source for parsing enum lists (feed formats, search formats/fields) — the
    /// table drives both the mapping and the "valid names" in errors.
    fn mapped<T: Copy>(&self, text: &str, table: &[(&'static str, T)]) -> Result<Vec<T>> {
        let mut seen: Vec<&'static str> = Vec::new();
        let mut out = Vec::new();
        for entry in self.entries().iter().filter(|e| e.name().is_none()) {
            let span = EntryExt::span(entry);
            let name = entry.value().as_str(text, span)?;
            let (canonical, value) = table
                .iter()
                .find(|(n, _)| *n == name)
                .ok_or_else(|| Keys::unknown(table, text, &name, span))?;
            // A repeated entry would silently emit the output twice.
            if seen.contains(canonical) {
                return Err(ConfigError::bad_value(
                    text,
                    format!("duplicate `{name}` in `{}`", self.name().value()),
                    span,
                )
                .into());
            }
            seen.push(canonical);
            out.push(*value);
        }
        Ok(out)
    }

    fn collections(&self, text: &str) -> Result<Vec<(String, CollectionConfig)>> {
        self.unique(text, "collection", |node, t| node.as_collection(t))
    }

    fn taxonomies(&self, text: &str) -> Result<Vec<(String, TaxonomyConfig)>> {
        self.unique(text, "taxonomy", |node, t| node.as_taxonomy(t))
    }

    fn html(&self, text: &str) -> Result<HtmlConfig> {
        const HTML: Block<HtmlConfig> = Block(&[
            ("pretty", |c, n, t| { c.pretty = n.boolean(t, 0)?; Ok(()) }),
            ("embed", |c, n, t| { c.embed = n.boolean(t, 0)?; Ok(()) }),
            ("meta", |c, n, t| { c.meta = n.boolean(t, 0)?; Ok(()) }),
        ]);
        let mut html = HtmlConfig::default();
        HTML.fill(&mut html, self, text)?;
        Ok(html)
    }

    /// The `images { lazy; optimize { … } }` section.
    fn images(&self, text: &str) -> Result<ImagesConfig> {
        const IMAGES: Block<ImagesConfig> = Block(&[
            ("lazy", |c, n, t| { c.lazy = n.boolean(t, 0)?; Ok(()) }),
            ("optimize", |c, n, t| { c.optimize = n.optimize(t)?; Ok(()) }),
        ]);
        let mut images = ImagesConfig::default();
        IMAGES.fill(&mut images, self, text)?;
        Ok(images)
    }

    /// The `optimize { png [level=…] [strip=…]; jpeg [quality=…] }` block: each
    /// child names a format (leniently — `jpg`/`jpeg` both work) and enables it,
    /// with optional per-format tuning as attributes.
    fn optimize(&self, text: &str) -> Result<OptimizeConfig> {
        let mut cfg = OptimizeConfig::default();
        for node in self.block(text)?.nodes() {
            let span = NodeExt::span(node);
            match node.name().value() {
                "png" => cfg.png = Some(node.png(text)?),
                "jpeg" | "jpg" => cfg.jpeg = Some(node.jpeg(text)?),
                other => {
                    return Err(ConfigError::bad_value(
                        text,
                        format!("unknown image format `{other}` (valid: png, jpeg)"),
                        span,
                    )
                    .into());
                }
            }
        }
        Ok(cfg)
    }

    fn png(&self, text: &str) -> Result<PngConfig> {
        const ATTRS: Attrs<PngConfig> = Attrs(&[
            ("level", |c, v, t, s| { c.level = v.integer(t, s)?.clamp(0, 6) as u8; Ok(()) }),
            ("strip", |c, v, t, s| {
                c.strip = v.one(t, s, &[
                    ("none", PngStrip::None),
                    ("safe", PngStrip::Safe),
                    ("all", PngStrip::All),
                ])?;
                Ok(())
            }),
        ]);
        let mut png = PngConfig::default();
        ATTRS.apply(&mut png, self, text, 0)?;
        Ok(png)
    }

    fn jpeg(&self, text: &str) -> Result<JpegConfig> {
        const ATTRS: Attrs<JpegConfig> = Attrs(&[
            ("quality", |c, v, t, s| { c.quality = v.integer(t, s)?.clamp(1, 100) as u8; Ok(()) }),
        ]);
        let mut jpeg = JpegConfig::default();
        ATTRS.apply(&mut jpeg, self, text, 0)?;
        Ok(jpeg)
    }

    fn assets(&self, text: &str) -> Result<AssetConfig> {
        const ASSETS: Block<AssetConfig> = Block(&[
            ("minify", |c, n, t| { c.minify = n.boolean(t, 0)?; Ok(()) }),
            ("bundle", |c, n, t| { c.bundle = n.boolean(t, 0)?; Ok(()) }),
            ("fingerprint", |c, n, t| { c.fingerprint = n.boolean(t, 0)?; Ok(()) }),
        ]);
        let mut asset = AssetConfig::default();
        ASSETS.fill(&mut asset, self, text)?;
        Ok(asset)
    }

    fn robots(&self, text: &str) -> Result<RobotsConfig> {
        const ROBOTS: Block<RobotsConfig> =
            Block(&[("disallow", |c, n, t| { c.disallow = n.words(t)?; Ok(()) })]);
        // Presence of the block enables emission.
        let mut robots = RobotsConfig {
            enabled: true,
            ..RobotsConfig::default()
        };
        ROBOTS.fill(&mut robots, self, text)?;
        Ok(robots)
    }

    fn llms(&self, text: &str) -> Result<LlmsConfig> {
        const LLMS: Block<LlmsConfig> =
            Block(&[("summary", |c, n, t| { c.summary = Some(n.string(t, 0)?); Ok(()) })]);
        // Presence of the block enables emission.
        let mut llms = LlmsConfig {
            enabled: true,
            ..LlmsConfig::default()
        };
        LLMS.fill(&mut llms, self, text)?;
        Ok(llms)
    }

    /// The `paths { … }` parent section: directory layout knobs.
    fn paths(&self, config: &mut Config, text: &str) -> Result<()> {
        const PATHS: Block<Config> = Block(&[
            ("content", |c, n, t| { c.content = n.string(t, 0)?.into(); Ok(()) }),
            ("dist", |c, n, t| { c.dist = n.string(t, 0)?.into(); Ok(()) }),
            ("assets", |c, n, t| { c.assets = n.string(t, 0)?.into(); Ok(()) }),
            ("templates", |c, n, t| { c.templates = n.string(t, 0)?.into(); Ok(()) }),
        ]);
        PATHS.fill(config, self, text)
    }

    /// The `typst { … }` parent section: typst engine knobs.
    fn typst(&self, config: &mut Config, text: &str) -> Result<()> {
        const TYPST: Block<Config> = Block(&[
            ("features", |c, n, t| { c.features = n.features(t)?; Ok(()) }),
            ("inputs", |c, n, t| { c.inputs = n.inputs(t)?; Ok(()) }),
        ]);
        TYPST.fill(config, self, text)
    }

    /// The `output { … }` parent section: everything the build emits.
    fn output(&self, config: &mut Config, text: &str) -> Result<()> {
        const OUTPUT: Block<Config> = Block(&[
            ("html", |c, n, t| { c.html = n.html(t)?; Ok(()) }),
            ("images", |c, n, t| { c.images = n.images(t)?; Ok(()) }),
            ("assets", |c, n, t| { c.asset = n.assets(t)?; Ok(()) }),
            ("sitemap", |c, n, t| { c.sitemap = n.boolean(t, 0)?; Ok(()) }),
            ("robots", |c, n, t| { c.robots = n.robots(t)?; Ok(()) }),
            ("llms", |c, n, t| { c.llms = n.llms(t)?; Ok(()) }),
            ("feed", |c, n, t| { c.feed = n.feed(t)?; Ok(()) }),
            ("search", |c, n, t| { c.search = n.search(t)?; Ok(()) }),
        ]);
        OUTPUT.fill(config, self, text)
    }

    fn draft(&self, text: &str) -> Result<DraftConfig> {
        const DRAFT: Block<DraftConfig> = Block(&[
            ("build", |c, n, t| { c.build = n.boolean(t, 0)?; Ok(()) }),
            ("suffix", |c, n, t| { c.suffix = n.string(t, 0)?; Ok(()) }),
        ]);
        let mut draft = DraftConfig::default();
        DRAFT.fill(&mut draft, self, text)?;
        Ok(draft)
    }

    fn links(&self, text: &str) -> Result<LinkConfig> {
        const LINKS: Block<LinkConfig> =
            Block(&[("strict", |c, n, t| { c.strict = n.boolean(t, 0)?; Ok(()) })]);
        let mut links = LinkConfig::default();
        LINKS.fill(&mut links, self, text)?;
        Ok(links)
    }

    fn feed(&self, text: &str) -> Result<FeedConfig> {
        /// Feed formats as `(name, kind)` — single source for parsing + errors.
        const FORMATS: &[(&str, FeedKind)] = &[("rss", FeedKind::Rss), ("atom", FeedKind::Atom)];
        const FEED: Block<FeedConfig> = Block(&[
            ("formats", |c, n, t| { c.formats = n.mapped(t, FORMATS)?; Ok(()) }),
            ("limit", |c, n, t| { c.limit = n.count(t, 0)?; Ok(()) }),
        ]);
        let mut feed = FeedConfig::default();
        FEED.fill(&mut feed, self, text)?;
        Ok(feed)
    }

    fn search(&self, text: &str) -> Result<SearchConfig> {
        /// Index formats as `(name, kind)` — single source for parsing + errors.
        const FORMATS: &[(&str, SearchFormat)] =
            &[("json", SearchFormat::Json), ("inverted", SearchFormat::Inverted)];
        /// Indexable page fields as `(name, field)`.
        const FIELDS: &[(&str, SearchField)] = &[
            ("title", SearchField::Title),
            ("body", SearchField::Body),
            ("tags", SearchField::Tags),
        ];
        const SEARCH: Block<SearchConfig> = Block(&[
            ("formats", |c, n, t| { c.formats = n.mapped(t, FORMATS)?; Ok(()) }),
            ("fields", |c, n, t| { c.fields = n.mapped(t, FIELDS)?; Ok(()) }),
            ("stopwords", |c, n, t| { c.stopwords = n.words(t)?; Ok(()) }),
            ("minimum", |c, n, t| { c.min_length = n.count(t, 0)?; Ok(()) }),
            ("client", |c, n, t| { c.client = n.boolean(t, 0)?; Ok(()) }),
        ]);
        let mut search = SearchConfig::default();
        SEARCH.fill(&mut search, self, text)?;
        Ok(search)
    }

    fn cache(&self, text: &str) -> Result<CacheConfig> {
        const CACHE: Block<CacheConfig> = Block(&[
            ("dir", |c, n, t| { c.dir = n.string(t, 0)?.into(); Ok(()) }),
            ("incremental", |c, n, t| { c.incremental = n.boolean(t, 0)?; Ok(()) }),
        ]);
        let mut cache = CacheConfig::default();
        CACHE.fill(&mut cache, self, text)?;
        Ok(cache)
    }

    fn hooks(&self, text: &str) -> Result<HooksConfig> {
        const HOOKS: Block<HooksConfig> = Block(&[
            ("before", |c, n, t| { c.before.extend(n.words(t)?); Ok(()) }),
            ("after", |c, n, t| { c.after.extend(n.words(t)?); Ok(()) }),
        ]);
        let mut hooks = HooksConfig::default();
        HOOKS.fill(&mut hooks, self, text)?;
        Ok(hooks)
    }

    fn serve(&self, text: &str) -> Result<ServeConfig> {
        const SERVE: Block<ServeConfig> = Block(&[
            ("port", |c, n, t| { c.port = n.port(t, 0)?; Ok(()) }),
            ("bind", |c, n, t| { c.bind = n.string(t, 0)?; Ok(()) }),
            ("open", |c, n, t| { c.open = n.boolean(t, 0)?; Ok(()) }),
            ("watch", |c, n, t| { c.watch = n.boolean(t, 0)?; Ok(()) }),
            ("include", |c, n, t| { c.include = n.words(t)?; Ok(()) }),
            ("exclude", |c, n, t| { c.exclude = n.words(t)?; Ok(()) }),
        ]);
        let mut serve = ServeConfig::default();
        SERVE.fill(&mut serve, self, text)?;
        Ok(serve)
    }

    fn profiles(&self, text: &str) -> Result<Vec<(String, KdlDocument)>> {
        self.unique(text, "profile", |child, t| {
            Ok((child.name().value().to_owned(), child.block(t)?.clone()))
        })
    }

    fn as_collection(&self, text: &str) -> Result<(String, CollectionConfig)> {
        const ATTRS: Attrs<CollectionConfig> = Attrs(&[
            ("sort", |c, v, t, s| { c.sort = v.sort(t, s)?; Ok(()) }),
            ("reverse", |c, v, t, s| { c.reverse = v.boolean(t, s)?; Ok(()) }),
            ("permalink", |c, v, t, s| {
                let raw = v.as_str(t, s)?;
                // Validate here so a template typo is a spanned config error,
                // not a silent fall-back to the convention at page load.
                Permalink::parse(&raw).map_err(|e| ConfigError::at(t, e.into(), s))?;
                c.permalink = Some(raw);
                Ok(())
            }),
            ("template", |c, v, t, s| { c.template = Some(v.as_str(t, s)?); Ok(()) }),
            ("paginate", |c, v, t, s| {
                let n = v.integer(t, s)?;
                if n < 1 {
                    return Err(ConfigError::bad_value(
                        t,
                        format!("paginate must be at least 1, got {n}"),
                        s,
                    )
                    .into());
                }
                c.paginate = Some(n as usize);
                Ok(())
            }),
            ("list", |c, v, t, s| { c.list = Some(v.as_str(t, s)?); Ok(()) }),
        ]);
        let mut cfg = CollectionConfig::default();
        // A leading positional argument is the collection's glob.
        if let Some(glob) = self.entries().first().filter(|e| e.name().is_none()) {
            cfg.glob = Some(glob.value().as_str(text, NodeExt::span(self))?);
        }
        ATTRS.apply(&mut cfg, self, text, 1)?;
        Ok((self.name().value().to_owned(), cfg))
    }

    fn as_taxonomy(&self, text: &str) -> Result<(String, TaxonomyConfig)> {
        const ATTRS: Attrs<TaxonomyConfig> = Attrs(&[
            ("key", |x, v, t, s| { x.key = v.as_str(t, s)?; Ok(()) }),
            ("index", |x, v, t, s| { x.index = v.boolean(t, s)?; Ok(()) }),
            ("template", |x, v, t, s| { x.template = Some(v.as_str(t, s)?); Ok(()) }),
        ]);
        let id = self.name().value().to_owned();
        let mut tax = TaxonomyConfig {
            key: id.clone(),
            index: false,
            template: None,
        };
        ATTRS.apply(&mut tax, self, text, 0)?;
        Ok((id, tax))
    }
}

/// Span bridging for a single [`KdlEntry`] (kdl's own miette 7 → ours), so
/// value-level errors can point at the exact argument rather than the node.
pub(super) trait EntryExt {
    fn span(&self) -> SourceSpan;
}

impl EntryExt for KdlEntry {
    fn span(&self) -> SourceSpan {
        let s = KdlEntry::span(self);
        SourceSpan::new(s.offset().into(), s.len())
    }
}
