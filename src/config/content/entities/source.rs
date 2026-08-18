//! `content { entities { <id> { sources } } }`: where a registry's entities
//! come from.
//!
//! Each source is a declaration here and a loader in
//! [`crate::content::entities::source`]. Sources are read in the order written,
//! and a later one *fills* an entity an earlier one declared rather than
//! replacing it.

use std::ops::Range;
use std::path::PathBuf;

use kdl::{KdlDocument, KdlNode};
use miette::SourceSpan;

use crate::codegen::Value;
use crate::config::dispatch::Kind::{Path, Tables};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::error::{ConfigError, Result};

/// One entity as either roster declares it: an id, the fields written under it,
/// and where each of them sits. The positions are ranges rather than
/// `SourceSpan`s so the config they live in stays hashable.
#[derive(Debug, Clone, Hash)]
pub struct Declared {
    pub id: String,
    pub fields: Vec<(String, Value)>,
    /// Where the entity's own node sits in the text it was read from.
    pub at: Range<usize>,
    /// Where each field sits, by key.
    pub spans: Vec<(String, Range<usize>)>,
}

impl Declared {
    /// The noun a duplicate id is reported as.
    const NOUN: &'static str = "entity";

    fn range(span: SourceSpan) -> Range<usize> {
        span.offset()..span.offset() + span.len()
    }

    /// One `zoe { name "Zoe" }` node.
    fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let id = node.name().value().to_owned();
        let fields = node.table(text)?;
        let spans = node
            .block(text)?
            .nodes()
            .iter()
            .map(|field| {
                (
                    field.name().value().to_owned(),
                    Self::range(NodeExt::span(field)),
                )
            })
            .collect();
        Ok((
            id.clone(),
            Self {
                id,
                fields,
                at: Self::range(NodeExt::span(node)),
                spans,
            },
        ))
    }

    /// Every entity written in a block of them, refusing a repeated id.
    fn block(node: &KdlNode, text: &str) -> Result<Vec<Self>> {
        Ok(node
            .unique(text, Self::NOUN, Self::item)?
            .into_iter()
            .map(|(_, declared)| declared)
            .collect())
    }

    /// Every entity written in a whole KDL document: a `data` roster, which is
    /// the `inline` block with the braces around it removed. Read through the
    /// same node reader, by hanging the document off a node nobody wrote.
    pub(crate) fn document(text: &str) -> Result<Vec<Self>> {
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        let mut node = KdlNode::new("entities");
        node.set_children(doc);
        Self::block(&node, text)
    }
}

/// A registry's sources, in the order they were declared.
#[derive(Debug, Clone, Default, Hash)]
pub struct SourcesConfig(pub Vec<SourceConfig>);

/// One declared source. The payload is what its loader reads.
#[derive(Debug, Clone, Hash)]
pub enum SourceConfig {
    Pages(PagesSource),
    Data(DataSource),
    Inline(InlineSource),
}

impl SourceConfig {
    /// How this source is spelled in config.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Pages(_) => "pages",
            Self::Data(_) => "data",
            Self::Inline(_) => "inline",
        }
    }
}

/// Entities are the content pages under a directory: their frontmatter is the
/// fields, their body is the prose.
#[derive(Debug, Clone, Hash)]
pub struct PagesSource {
    /// The directory, relative to the project root.
    pub dir: PathBuf,
}

/// Entities are the KDL nodes of a file: a roster checked in beside the
/// content.
#[derive(Debug, Clone, Hash)]
pub struct DataSource {
    /// The file, relative to the project root.
    pub path: PathBuf,
}

/// Entities are written in the config itself.
#[derive(Debug, Clone, Hash)]
pub struct InlineSource {
    pub entities: Vec<Declared>,
}

impl SourcesConfig {
    /// Every source written under `key`, which is how a row that appends to one
    /// list reads back.
    fn under(&self, key: &str) -> crate::config::Value {
        self.0
            .iter()
            .filter(|source| source.name() == key)
            .map(crate::config::Value::from)
            .collect()
    }
}

/// A source as its own line spells it: the path it names, or the ids an inline
/// block declares.
impl From<&SourceConfig> for crate::config::Value {
    fn from(source: &SourceConfig) -> Self {
        match source {
            SourceConfig::Pages(pages) => pages.dir.clone().into(),
            SourceConfig::Data(data) => data.path.clone().into(),
            SourceConfig::Inline(inline) => inline
                .entities
                .iter()
                .map(|entity| entity.id.clone())
                .collect(),
        }
    }
}

/// The `sources { .. }` block: every key appends, so it reads top to bottom,
/// and the block as a whole replaces whatever a base config declared.
impl Section for SourcesConfig {
    const RULES: Block<Self> = Block(&[
        (
            "pages",
            Path,
            "A directory of profile pages: each page's frontmatter is one entity's fields.",
            |c| c.under("pages"),
            |c, n, t| {
                c.0.push(SourceConfig::Pages(PagesSource {
                    dir: PathBuf::from(n.string(t, 0)?),
                }));
                Ok(())
            },
        ),
        (
            "data",
            Path,
            "A KDL roster file, written exactly as an `inline` block is.",
            |c| c.under("data"),
            |c, n, t| {
                c.0.push(SourceConfig::Data(DataSource {
                    path: PathBuf::from(n.string(t, 0)?),
                }));
                Ok(())
            },
        ),
        (
            "inline",
            Tables,
            "Entities written here, one block per id, each holding its fields.",
            |c| c.under("inline"),
            |c, n, t| {
                c.0.push(SourceConfig::Inline(InlineSource {
                    entities: Declared::block(n, t)?,
                }));
                Ok(())
            },
        ),
    ]);
}
