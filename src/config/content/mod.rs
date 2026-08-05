//! `content { }`: what the content tree holds and how it is read.

pub mod collection;
pub mod drafts;
pub mod markdown;
pub mod taxonomy;

use crate::config::dispatch::Kind::{Block as Nested, Flag, Items, Lines, Text};
use crate::config::dispatch::{Attributed, Block, Section};
use crate::config::node::NodeExt;
use crate::config::{CollectionConfig, DraftConfig, MarkdownConfig, TaxonomyConfig};
use crate::error::{ConfigError, ConfigErrorKind};

/// What the content tree holds and how it is read. The directory itself is
/// [`Paths::content`](crate::config::Paths::content); everything here is about
/// the pages inside it.
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
    pub drafts: DraftConfig,
    /// Collection overrides keyed by id.
    pub collections: Vec<(String, CollectionConfig)>,
    /// Taxonomy definitions.
    pub taxonomies: Vec<(String, TaxonomyConfig)>,
    /// How markdown pages are read.
    pub markdown: MarkdownConfig,
}

impl Default for ContentConfig {
    fn default() -> Self {
        Self {
            index: Some("index".into()),
            future: false,
            drafts: DraftConfig::default(),
            collections: Vec::default(),
            taxonomies: Vec::default(),
            markdown: MarkdownConfig::default(),
        }
    }
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
            "drafts",
            Nested(DraftConfig::rows),
            "Whether drafts are built, and how one is marked. `drafts #true` is `drafts { build #true }`.",
            |c, n, t| c.drafts.shorthand(n, t, "build"),
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
        (
            "markdown",
            Nested(MarkdownConfig::rows),
            "Whether `.md` files are pages, and what one may contain. `markdown #false` is `markdown { enabled #false }`.",
            |c, n, t| c.markdown.shorthand(n, t, "enabled"),
        ),
    ]);
}
