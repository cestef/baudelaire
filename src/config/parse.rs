use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::SourceSpan;

use crate::config::dispatch::{Attrs, Block, Keys};
use crate::config::value::ValueExt;
use crate::config::{
    AssetConfig, CacheConfig, CollectionConfig, Config, DraftConfig, FeedConfig, FeedKind,
    HooksConfig, HtmlConfig, ImagesConfig, JpegConfig, LinkConfig, LlmsConfig, OptimizeConfig,
    DeployConfig, PngConfig, PngStrip, AnnounceConfig, RobotsConfig, SearchConfig, SearchField, SearchFormat,
    S3Config, ServeConfig, SshConfig, StandardConfig, TaxonomyConfig, VerifyConfig,
};
use crate::content::Permalink;
use crate::error::{ConfigError, Result};

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        // keep the raw text: profile overlay reports errors against it, its nodes carry spans into it
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
        ("paths", |c, n, t| n.paths(c, t)),
        ("future", |c, n, t| {
            c.future = n.boolean(t, 0)?;
            Ok(())
        }),
        ("draft", |c, n, t| n.draft(&mut c.draft, t)),
        ("links", |c, n, t| n.links(&mut c.links, t)),
        ("cache", |c, n, t| n.cache(&mut c.cache, t)),
        ("typst", |c, n, t| n.typst(c, t)),
        ("output", |c, n, t| n.output(c, t)),
        ("collections", |c, n, t| {
            c.collections = n.collections(t)?;
            Ok(())
        }),
        ("taxonomies", |c, n, t| {
            c.taxonomies = n.taxonomies(t)?;
            Ok(())
        }),
        ("client", |c, n, t| {
            c.client = n.client(t)?;
            Ok(())
        }),
        ("hooks", |c, n, t| n.hooks(&mut c.hooks, t)),
        ("announce", |c, n, t| n.announce(&mut c.announce, t)),
        ("deploy", |c, n, t| n.deploy(&mut c.deploy, t)),
        ("serve", |c, n, t| n.serve(&mut c.serve, t)),
        ("profiles", |c, n, t| {
            c.profiles = n.profiles(t)?;
            Ok(())
        }),
    ]);
}

/// Typed accessors over a [`KdlNode`], plus the parent-section builders that map
/// a `{ .. }` block onto its config struct. Shared with [`super::dispatch`],
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
    /// The node's `{ .. }` children parsed as `(id, item)` pairs, erroring on a
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
    fn client(&self, text: &str) -> Result<Vec<(String, crate::codegen::Value)>>;
    fn features(&self, text: &str) -> Result<Vec<String>>;
    fn words(&self, text: &str) -> Result<Vec<String>>;
    fn mapped<T: Copy>(&self, text: &str, table: &[(&'static str, T)]) -> Result<Vec<T>>;
    fn collections(&self, text: &str) -> Result<Vec<(String, CollectionConfig)>>;
    fn taxonomies(&self, text: &str) -> Result<Vec<(String, TaxonomyConfig)>>;
    fn as_collection(&self, text: &str) -> Result<(String, CollectionConfig)>;
    fn as_taxonomy(&self, text: &str) -> Result<(String, TaxonomyConfig)>;
    fn html(&self, target: &mut HtmlConfig, text: &str) -> Result<()>;
    fn images(&self, target: &mut ImagesConfig, text: &str) -> Result<()>;
    fn optimize(&self, target: &mut OptimizeConfig, text: &str) -> Result<()>;
    fn png(&self, target: &mut PngConfig, text: &str) -> Result<()>;
    fn jpeg(&self, target: &mut JpegConfig, text: &str) -> Result<()>;
    fn assets(&self, target: &mut AssetConfig, text: &str) -> Result<()>;
    fn robots(&self, target: &mut RobotsConfig, text: &str) -> Result<()>;
    fn llms(&self, target: &mut LlmsConfig, text: &str) -> Result<()>;
    fn draft(&self, target: &mut DraftConfig, text: &str) -> Result<()>;
    fn links(&self, target: &mut LinkConfig, text: &str) -> Result<()>;
    fn feed(&self, target: &mut FeedConfig, text: &str) -> Result<()>;
    fn search(&self, target: &mut SearchConfig, text: &str) -> Result<()>;
    fn cache(&self, target: &mut CacheConfig, text: &str) -> Result<()>;
    fn hooks(&self, target: &mut HooksConfig, text: &str) -> Result<()>;
    fn announce(&self, target: &mut AnnounceConfig, text: &str) -> Result<()>;
    fn standard(&self, target: &mut Option<StandardConfig>, text: &str) -> Result<()>;
    fn deploy(&self, target: &mut DeployConfig, text: &str) -> Result<()>;
    fn s3(&self, target: &mut Option<S3Config>, text: &str) -> Result<()>;
    fn ssh(&self, target: &mut Option<SshConfig>, text: &str) -> Result<()>;
    fn verify(&self, target: &mut VerifyConfig, text: &str) -> Result<()>;
    fn serve(&self, target: &mut ServeConfig, text: &str) -> Result<()>;
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
            None => {
                Err(ConfigError::missing_arg(text, self.name().value(), NodeExt::span(self)).into())
            }
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
            ConfigError::negative_count(text, self.name().value(), n, NodeExt::span(self)).into()
        })
    }

    /// A TCP port argument, range-checked so `port 99999` errors instead of
    /// wrapping to a different port.
    fn port(&self, text: &str, idx: usize) -> Result<u16> {
        let n = self.int(text, idx)?;
        u16::try_from(n).map_err(|_| ConfigError::port_range(text, n, NodeExt::span(self)).into())
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
                return Err(ConfigError::duplicate_id(text, noun, &id, NodeExt::span(node)).into());
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

    fn client(&self, text: &str) -> Result<Vec<(String, crate::codegen::Value)>> {
        let children = self.block(text)?;
        children
            .nodes()
            .iter()
            .map(|child| {
                let value = child.arg(text, 0)?.scalar(text, NodeExt::span(child))?;
                Ok((child.name().value().to_owned(), value))
            })
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
                // a `-` prefix reads as "disable", but nothing subtracts — error beats silently enabling
                if let Some(name) = raw.strip_prefix('-') {
                    return Err(
                        ConfigError::feature_removal(text, name, NodeExt::span(self)).into(),
                    );
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
                .ok_or_else(|| Keys::unknown_value(table, text, &name, span))?;
            // a repeated entry would silently emit the output twice
            if seen.contains(canonical) {
                return Err(
                    ConfigError::duplicate_entry(text, &name, self.name().value(), span).into(),
                );
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

    fn html(&self, target: &mut HtmlConfig, text: &str) -> Result<()> {
        const HTML: Block<HtmlConfig> = Block(&[
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
        ]);
        HTML.fill(target, self, text)
    }

    /// The `images { lazy; optimize { .. } }` section.
    fn images(&self, target: &mut ImagesConfig, text: &str) -> Result<()> {
        const IMAGES: Block<ImagesConfig> = Block(&[
            ("lazy", |c, n, t| {
                c.lazy = n.boolean(t, 0)?;
                Ok(())
            }),
            ("optimize", |c, n, t| n.optimize(&mut c.optimize, t)),
        ]);
        IMAGES.fill(target, self, text)
    }

    /// The `optimize { png [level=..] [strip=..]; jpeg [quality=..] }` block: each
    /// child names a format (leniently — `jpg`/`jpeg` both work) and enables it,
    /// with optional per-format tuning as attributes. Fills onto the existing
    /// per-format config so a profile tuning one attribute keeps its siblings.
    fn optimize(&self, target: &mut OptimizeConfig, text: &str) -> Result<()> {
        for node in self.block(text)?.nodes() {
            let span = NodeExt::span(node);
            match node.name().value() {
                "png" => node.png(target.png.get_or_insert_default(), text)?,
                "jpeg" | "jpg" => node.jpeg(target.jpeg.get_or_insert_default(), text)?,
                other => {
                    return Err(ConfigError::unknown_image_format(text, other, span).into());
                }
            }
        }
        Ok(())
    }

    fn png(&self, target: &mut PngConfig, text: &str) -> Result<()> {
        const ATTRS: Attrs<PngConfig> = Attrs(&[
            ("level", |c, v, t, s| {
                c.level = v.ranged(t, s, 0, 6)? as u8;
                Ok(())
            }),
            ("strip", |c, v, t, s| {
                c.strip = v.one(
                    t,
                    s,
                    &[
                        ("none", PngStrip::None),
                        ("safe", PngStrip::Safe),
                        ("all", PngStrip::All),
                    ],
                )?;
                Ok(())
            }),
        ]);
        ATTRS.apply(target, self, text, 0)
    }

    fn jpeg(&self, target: &mut JpegConfig, text: &str) -> Result<()> {
        const ATTRS: Attrs<JpegConfig> = Attrs(&[("quality", |c, v, t, s| {
            c.quality = v.ranged(t, s, 1, 100)? as u8;
            Ok(())
        })]);
        ATTRS.apply(target, self, text, 0)
    }

    fn assets(&self, target: &mut AssetConfig, text: &str) -> Result<()> {
        const ASSETS: Block<AssetConfig> = Block(&[
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
        ]);
        ASSETS.fill(target, self, text)
    }

    fn robots(&self, target: &mut RobotsConfig, text: &str) -> Result<()> {
        const ROBOTS: Block<RobotsConfig> = Block(&[("disallow", |c, n, t| {
            c.disallow = n.words(t)?;
            Ok(())
        })]);
        // presence of the block enables emission
        target.enabled = true;
        ROBOTS.fill(target, self, text)
    }

    fn llms(&self, target: &mut LlmsConfig, text: &str) -> Result<()> {
        const LLMS: Block<LlmsConfig> = Block(&[("summary", |c, n, t| {
            c.summary = Some(n.string(t, 0)?);
            Ok(())
        })]);
        // presence of the block enables emission
        target.enabled = true;
        LLMS.fill(target, self, text)
    }

    /// The `paths { .. }` parent section: directory layout knobs.
    fn paths(&self, config: &mut Config, text: &str) -> Result<()> {
        const PATHS: Block<Config> = Block(&[
            ("content", |c, n, t| {
                c.content = n.string(t, 0)?.into();
                Ok(())
            }),
            ("index", |c, n, t| {
                let s = n.string(t, 0)?;
                c.index = (!s.is_empty()).then_some(s);
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
        PATHS.fill(config, self, text)
    }

    /// The `typst { .. }` parent section: typst engine knobs.
    fn typst(&self, config: &mut Config, text: &str) -> Result<()> {
        const TYPST: Block<Config> = Block(&[
            ("features", |c, n, t| {
                c.features = n.features(t)?;
                Ok(())
            }),
            ("inputs", |c, n, t| {
                c.inputs = n.inputs(t)?;
                Ok(())
            }),
        ]);
        TYPST.fill(config, self, text)
    }

    /// The `output { .. }` parent section: everything the build emits.
    fn output(&self, config: &mut Config, text: &str) -> Result<()> {
        const OUTPUT: Block<Config> = Block(&[
            ("urls", |c, n, t| {
                c.urls = n.arg(t, 0)?.urls(t, NodeExt::span(n))?;
                Ok(())
            }),
            ("clean", |c, n, t| {
                c.clean = n.boolean(t, 0)?;
                Ok(())
            }),
            ("html", |c, n, t| n.html(&mut c.html, t)),
            ("images", |c, n, t| n.images(&mut c.images, t)),
            ("assets", |c, n, t| n.assets(&mut c.asset, t)),
            ("sitemap", |c, n, t| {
                c.sitemap = n.boolean(t, 0)?;
                Ok(())
            }),
            ("robots", |c, n, t| n.robots(&mut c.robots, t)),
            ("llms", |c, n, t| n.llms(&mut c.llms, t)),
            ("feed", |c, n, t| n.feed(&mut c.feed, t)),
            ("search", |c, n, t| n.search(&mut c.search, t)),
        ]);
        OUTPUT.fill(config, self, text)
    }

    fn draft(&self, target: &mut DraftConfig, text: &str) -> Result<()> {
        const DRAFT: Block<DraftConfig> = Block(&[
            ("build", |c, n, t| {
                c.build = n.boolean(t, 0)?;
                Ok(())
            }),
            ("suffix", |c, n, t| {
                c.suffix = n.string(t, 0)?;
                Ok(())
            }),
        ]);
        DRAFT.fill(target, self, text)
    }

    fn links(&self, target: &mut LinkConfig, text: &str) -> Result<()> {
        const LINKS: Block<LinkConfig> = Block(&[("strict", |c, n, t| {
            c.strict = n.boolean(t, 0)?;
            Ok(())
        })]);
        LINKS.fill(target, self, text)
    }

    fn feed(&self, target: &mut FeedConfig, text: &str) -> Result<()> {
        /// Feed formats as `(name, kind)` — single source for parsing + errors.
        const FORMATS: &[(&str, FeedKind)] = &[
            ("rss", FeedKind::Rss),
            ("atom", FeedKind::Atom),
            ("json", FeedKind::Json),
        ];
        const FEED: Block<FeedConfig> = Block(&[
            ("formats", |c, n, t| {
                c.formats = n.mapped(t, FORMATS)?;
                Ok(())
            }),
            ("limit", |c, n, t| {
                c.limit = n.count(t, 0)?;
                Ok(())
            }),
        ]);
        FEED.fill(target, self, text)
    }

    fn search(&self, target: &mut SearchConfig, text: &str) -> Result<()> {
        /// Index formats as `(name, kind)` — single source for parsing + errors.
        const FORMATS: &[(&str, SearchFormat)] = &[
            ("json", SearchFormat::Json),
            ("inverted", SearchFormat::Inverted),
        ];
        /// Indexable page fields as `(name, field)`.
        const FIELDS: &[(&str, SearchField)] = &[
            ("title", SearchField::Title),
            ("body", SearchField::Body),
            ("tags", SearchField::Tags),
        ];
        const SEARCH: Block<SearchConfig> = Block(&[
            ("formats", |c, n, t| {
                c.formats = n.mapped(t, FORMATS)?;
                Ok(())
            }),
            ("fields", |c, n, t| {
                c.fields = n.mapped(t, FIELDS)?;
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
            ("client", |c, n, t| {
                c.client = n.boolean(t, 0)?;
                Ok(())
            }),
        ]);
        SEARCH.fill(target, self, text)
    }

    fn cache(&self, target: &mut CacheConfig, text: &str) -> Result<()> {
        const CACHE: Block<CacheConfig> = Block(&[
            ("dir", |c, n, t| {
                c.dir = n.string(t, 0)?.into();
                Ok(())
            }),
            ("incremental", |c, n, t| {
                c.incremental = n.boolean(t, 0)?;
                Ok(())
            }),
        ]);
        CACHE.fill(target, self, text)
    }

    /// Each list replaces (never appends) so a profile overriding `before`
    /// leaves `after` inherited from the base — same whole-value replacement as
    /// every other list field.
    fn hooks(&self, target: &mut HooksConfig, text: &str) -> Result<()> {
        const HOOKS: Block<HooksConfig> = Block(&[
            ("before", |c, n, t| {
                c.before = n.words(t)?;
                Ok(())
            }),
            ("after", |c, n, t| {
                c.after = n.words(t)?;
                Ok(())
            }),
        ]);
        HOOKS.fill(target, self, text)
    }

    /// The `announce { .. }` parent section: one block per destination backend.
    fn announce(&self, target: &mut AnnounceConfig, text: &str) -> Result<()> {
        const ANNOUNCE: Block<AnnounceConfig> =
            Block(&[("standard", |c, n, t| n.standard(&mut c.standard, t))]);
        ANNOUNCE.fill(target, self, text)
    }

    /// The `deploy { .. }` parent section: one block per destination backend.
    fn deploy(&self, target: &mut DeployConfig, text: &str) -> Result<()> {
        const DEPLOY: Block<DeployConfig> = Block(&[
            ("s3", |c, n, t| n.s3(&mut c.s3, t)),
            ("ssh", |c, n, t| n.ssh(&mut c.ssh, t)),
        ]);
        DEPLOY.fill(target, self, text)
    }

    /// The `s3 { .. }` block: presence enables the S3 backend. Fills onto the
    /// existing config so a profile tuning one key keeps the rest.
    fn s3(&self, target: &mut Option<S3Config>, text: &str) -> Result<()> {
        const S3: Block<S3Config> = Block(&[
            ("bucket", |c, n, t| {
                c.bucket = n.string(t, 0)?;
                Ok(())
            }),
            ("endpoint", |c, n, t| {
                c.endpoint = Some(n.string(t, 0)?);
                Ok(())
            }),
            ("region", |c, n, t| {
                c.region = n.string(t, 0)?;
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
        let mut cfg = target.take().unwrap_or_default();
        S3.fill(&mut cfg, self, text)?;
        *target = Some(cfg);
        Ok(())
    }

    /// The `ssh { .. }` block: presence enables the SSH backend. Fills onto the
    /// existing config so a profile tuning one key keeps the rest.
    fn ssh(&self, target: &mut Option<SshConfig>, text: &str) -> Result<()> {
        const SSH: Block<SshConfig> = Block(&[
            ("host", |c, n, t| {
                c.host = n.string(t, 0)?;
                Ok(())
            }),
            ("path", |c, n, t| {
                c.path = n.string(t, 0)?;
                Ok(())
            }),
            ("port", |c, n, t| {
                c.port = n.count(t, 0)? as u16;
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
            ("delete", |c, n, t| {
                c.delete = n.boolean(t, 0)?;
                Ok(())
            }),
        ]);
        let mut cfg = target.take().unwrap_or_default();
        SSH.fill(&mut cfg, self, text)?;
        *target = Some(cfg);
        Ok(())
    }

    /// The `standard { .. }` block: presence enables the standard.site backend.
    /// Fills onto the existing config so a profile tuning one key keeps the rest.
    fn standard(&self, target: &mut Option<StandardConfig>, text: &str) -> Result<()> {
        const STANDARD: Block<StandardConfig> = Block(&[
            ("handle", |c, n, t| {
                c.handle = n.string(t, 0)?;
                Ok(())
            }),
            ("did", |c, n, t| {
                c.did = Some(n.string(t, 0)?);
                Ok(())
            }),
            ("pds", |c, n, t| {
                c.pds = n.string(t, 0)?;
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
            ("verify", |c, n, t| n.verify(&mut c.verify, t)),
        ]);
        let mut cfg = target.take().unwrap_or_default();
        STANDARD.fill(&mut cfg, self, text)?;
        *target = Some(cfg);
        Ok(())
    }

    /// The `verify { wellknown; links }` block: which build-time verification
    /// artifacts to emit.
    fn verify(&self, target: &mut VerifyConfig, text: &str) -> Result<()> {
        const VERIFY: Block<VerifyConfig> = Block(&[
            ("wellknown", |c, n, t| {
                c.wellknown = n.boolean(t, 0)?;
                Ok(())
            }),
            ("links", |c, n, t| {
                c.links = n.boolean(t, 0)?;
                Ok(())
            }),
        ]);
        VERIFY.fill(target, self, text)
    }

    fn serve(&self, target: &mut ServeConfig, text: &str) -> Result<()> {
        const SERVE: Block<ServeConfig> = Block(&[
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
        SERVE.fill(target, self, text)
    }

    fn profiles(&self, text: &str) -> Result<Vec<(String, KdlDocument)>> {
        self.unique(text, "profile", |child, t| {
            Ok((child.name().value().to_owned(), child.block(t)?.clone()))
        })
    }

    fn as_collection(&self, text: &str) -> Result<(String, CollectionConfig)> {
        const ATTRS: Attrs<CollectionConfig> = Attrs(&[
            ("sort", |c, v, t, s| {
                c.sort = v.sort(t, s)?;
                Ok(())
            }),
            ("reverse", |c, v, t, s| {
                c.reverse = v.boolean(t, s)?;
                Ok(())
            }),
            ("permalink", |c, v, t, s| {
                let raw = v.as_str(t, s)?;
                // validate here so a template typo is a spanned config error,
                // not a silent fallback to convention at page load
                Permalink::parse(&raw).map_err(|e| ConfigError::at(t, e.into(), s))?;
                c.permalink = Some(raw);
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
            ("list", |c, v, t, s| {
                c.list = Some(v.as_str(t, s)?);
                Ok(())
            }),
            ("index", |c, v, t, s| {
                c.index = Some(v.as_str(t, s)?);
                Ok(())
            }),
            ("prefix", |c, v, t, s| {
                c.prefix = v.as_str(t, s)?;
                Ok(())
            }),
        ]);
        let mut cfg = CollectionConfig::default();
        // a leading positional argument is the collection's glob
        if let Some(glob) = self.entries().first().filter(|e| e.name().is_none()) {
            cfg.glob = Some(glob.value().as_str(text, NodeExt::span(self))?);
        }
        ATTRS.apply(&mut cfg, self, text, 1)?;
        Ok((self.name().value().to_owned(), cfg))
    }

    fn as_taxonomy(&self, text: &str) -> Result<(String, TaxonomyConfig)> {
        const ATTRS: Attrs<TaxonomyConfig> = Attrs(&[
            ("key", |x, v, t, s| {
                x.key = v.as_str(t, s)?;
                Ok(())
            }),
            ("index", |x, v, t, s| {
                x.index = v.boolean(t, s)?;
                Ok(())
            }),
            ("template", |x, v, t, s| {
                x.template = Some(v.as_str(t, s)?);
                Ok(())
            }),
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
