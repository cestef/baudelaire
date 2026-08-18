//! `content { }`: what the content tree holds and how it is read.

pub mod collection;
pub mod drafts;
pub mod entities;
pub mod markdown;
pub mod reading;
pub mod taxonomy;

use crate::config::dispatch::Kind::{Block as Nested, Flag, Items, Lines, Text};
use crate::config::dispatch::{Attributed, Block, Section};
use crate::config::node::NodeExt;
use crate::config::{
    CollectionConfig, DraftConfig, MarkdownConfig, ReadingConfig, RegistryConfig, TaxonomyConfig,
    Value,
};
use crate::error::{ConfigError, ConfigErrorKind};

/// What the content tree holds and how it is read; the directory itself is
/// [`Paths::content`](crate::config::Paths::content).
#[derive(Debug, Clone, Hash)]
pub struct ContentConfig {
    /// Bundle index basename: a file with this stem takes its slug from its
    /// parent directory, so `posts/hello/index.typ` becomes `/posts/hello/`.
    /// `None` keys every page by its own filename.
    pub index: Option<String>,
    /// Build future-dated posts.
    pub future: bool,
    pub drafts: DraftConfig,
    pub collections: Vec<(String, CollectionConfig)>,
    pub taxonomies: Vec<(String, TaxonomyConfig)>,
    /// The registries a taxonomy's terms resolve into, keyed by id.
    pub entities: Vec<(String, RegistryConfig)>,
    pub markdown: MarkdownConfig,
    pub reading: ReadingConfig,
}

impl Default for ContentConfig {
    fn default() -> Self {
        Self {
            index: Some("index".into()),
            future: false,
            drafts: DraftConfig::default(),
            collections: Vec::default(),
            taxonomies: Vec::default(),
            entities: Vec::default(),
            markdown: MarkdownConfig::default(),
            reading: ReadingConfig::default(),
        }
    }
}

impl Section for ContentConfig {
    const RULES: Block<Self> = Block(&[
        (
            "index",
            Text,
            "The filename stem that publishes at its directory's own URL, without extension.",
            |c| c.index.clone().into(),
            |c, n, t| {
                let stem = n.string(t, 0)?;
                let named = crate::config::Config::SOURCES.iter().find_map(|ext| {
                    stem.strip_suffix(&format!(".{ext}"))
                        .filter(|bare| !bare.is_empty())
                });
                if let Some(bare) = named {
                    return Err(ConfigError::at(
                        t,
                        ConfigErrorKind::IndexExtension {
                            got: stem.clone(),
                            stem: bare.to_owned(),
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
            |c| c.future.into(),
            |c, n, t| {
                c.future = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "drafts",
            Nested(DraftConfig::rows),
            "Whether drafts are built, and how one is marked. `drafts #true` is `drafts { build #true }`.",
            |c| c.drafts.values(),
            |c, n, t| c.drafts.shorthand(n, t, "build"),
        ),
        (
            "collections",
            Items(CollectionConfig::rows),
            "One block per collection, each named by its id.",
            |c| Value::each(&c.collections, Section::values),
            |c, n, t| {
                c.collections = n.unique(t, "collection", CollectionConfig::item)?;
                Ok(())
            },
        ),
        (
            "taxonomies",
            Lines(TaxonomyConfig::rows),
            "One line per taxonomy, each named by its id.",
            |c| Value::each(&c.taxonomies, Attributed::values),
            |c, n, t| {
                c.taxonomies = n.unique(t, "taxonomy", TaxonomyConfig::item)?;
                Ok(())
            },
        ),
        (
            "entities",
            Items(RegistryConfig::rows),
            "One block per registry, each named by its id: what its entities carry and where they come from.",
            |c| Value::each(&c.entities, Section::values),
            |c, n, t| {
                c.entities = n.unique(t, "registry", RegistryConfig::item)?;
                Ok(())
            },
        ),
        (
            "reading",
            Nested(ReadingConfig::rows),
            "How a page's reading estimate is measured.",
            |c| c.reading.values(),
            |c, n, t| c.reading.fill(n, t),
        ),
        (
            "markdown",
            Nested(MarkdownConfig::rows),
            "Whether `.md` files are pages, and what one may contain. `markdown #false` is `markdown { enabled #false }`.",
            |c| c.markdown.values(),
            |c, n, t| c.markdown.shorthand(n, t, "enabled"),
        ),
    ]);
}
