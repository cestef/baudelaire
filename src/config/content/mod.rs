//! `content { }`: what the content tree holds and how it is read.

pub mod collection;
pub mod drafts;
pub mod entities;
pub mod history;
pub mod markdown;
pub mod reading;
pub mod taxonomy;

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;
use crate::config::{
    CollectionConfig, DraftConfig, HistoryConfig, MarkdownConfig, ReadingConfig, RegistryConfig,
    TaxonomyConfig,
};
use crate::error::{ConfigError, ConfigErrorKind};

/// What the content tree holds and how it is read; the directory itself is
/// [`Paths::content`](crate::config::Paths::content).
#[derive(Debug, Clone, Hash, Table)]
pub struct ContentConfig {
    /// The filename stem that publishes at its directory's own URL, without extension.
    ///
    /// A file with this stem takes its slug from its parent directory, so
    /// `posts/hello/index.typ` becomes `/posts/hello/`. `None` keys every page
    /// by its own filename.
    #[key(custom(
        Text,
        |c: &Self| c.index.clone().into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
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
    ))]
    pub index: Option<String>,

    /// Build pages dated later than now.
    #[key(flag)]
    pub future: bool,

    /// Whether drafts are built, and how one is marked. `drafts #true` is `drafts { build #true }`.
    #[key(shorthand(DraftConfig, "build"))]
    pub drafts: DraftConfig,

    /// One block per collection, each named by its id.
    #[key(items(CollectionConfig, "collection", CollectionConfig::item))]
    pub collections: Vec<(String, CollectionConfig)>,

    /// One block per taxonomy, each named by its id.
    #[key(items(TaxonomyConfig, "taxonomy", TaxonomyConfig::item))]
    pub taxonomies: Vec<(String, TaxonomyConfig)>,

    /// One block per registry, each named by its id: what its entities carry and where they come from.
    ///
    /// The registries a taxonomy's terms resolve into, keyed by id.
    #[key(items(RegistryConfig, "registry", RegistryConfig::item))]
    pub entities: Vec<(String, RegistryConfig)>,

    /// How a page's reading estimate is measured.
    #[key(nested(ReadingConfig))]
    pub reading: ReadingConfig,

    /// Whether `.md` files are pages, and what one may contain. `markdown #false` is `markdown { enabled #false }`.
    #[key(shorthand(MarkdownConfig, "enabled"))]
    pub markdown: MarkdownConfig,

    /// What git knows about each page, as `page.git`. Its presence turns it on.
    #[key(nested(HistoryConfig))]
    pub history: HistoryConfig,
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
            reading: ReadingConfig::default(),
            markdown: MarkdownConfig::default(),
            history: HistoryConfig::default(),
        }
    }
}
