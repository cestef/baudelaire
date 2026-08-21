//! `content { taxonomies { } }`: a term key, the pages it groups, and the
//! listings it generates over them.

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Number;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::vocab::rule;
use crate::config::{PaginateConfig, SortKey};
use crate::content::Credit;
use crate::error::{ConfigError, ConfigErrorKind, Result};
use crate::ui::markup;

#[derive(Debug, Clone, Hash, Table)]
pub struct TaxonomyConfig {
    /// The frontmatter field its terms are read from. Defaults to the taxonomy's own id.
    #[key(text)]
    pub key: String,

    /// The `content { entities { } }` registry its terms are ids in.
    ///
    /// `None` is a plain taxonomy, whose terms are words.
    #[key(opt text)]
    pub entities: Option<String>,

    /// What a page claims about the entities it names here, for the surfaces that can spell it.
    ///
    /// `authors` credits them with writing it. Only meaningful with
    /// `entities`.
    #[key(opt choice(Credit))]
    pub credit: Option<Credit>,

    /// Generate a page per term, and an index of the terms. Its presence turns them on; `#false` turns them off again.
    #[key(nested(ListingConfig))]
    pub listing: ListingConfig,

    /// Let a term written as a profile page be described by it, instead of generating a listing beside it.
    #[key(flag)]
    pub describe: bool,

    /// What a term's members are ordered by. Defaults to `title`, since a term spans collections.
    #[key(choice(SortKey))]
    pub sort: SortKey,

    /// Reverse that order, for the newest-first a dated term listing wants.
    #[key(flag)]
    pub reverse: bool,
}

/// A taxonomy's generated listings: whether there are any, how a term page is
/// chunked, and what renders it.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct ListingConfig {
    /// Whether the term pages and their index are generated at all: the
    /// block's presence.
    pub enabled: bool,

    /// Members per term page. Omitted, a term's listing is one page.
    #[key(custom(
        Number,
        |c: &Self| c.size.into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let written = n.arg(t, 0)?.integer(t, NodeExt::span(n))?;
            c.size = Some(PaginateConfig::size(written, t, NodeExt::span(n))?);
            Ok(())
        },
    ))]
    pub size: Option<usize>,

    /// The layout those listings render through.
    #[key(opt text)]
    pub template: Option<String>,

    /// The path segment before a term page's number, as in `/tags/rust/page/2/`.
    #[key(segment)]
    pub prefix: String,
}

impl Default for ListingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            size: None,
            template: None,
            prefix: PaginateConfig::PREFIX.into(),
        }
    }
}

/// A taxonomy's defaults depend on its id, since it reads the frontmatter key
/// of that name unless it names another: the conversion *is* the `Default` impl
/// it cannot have.
impl From<String> for TaxonomyConfig {
    fn from(id: String) -> Self {
        Self {
            key: id,
            entities: None,
            credit: None,
            listing: ListingConfig::default(),
            describe: false,
            sort: SortKey::Title,
            reverse: false,
        }
    }
}

impl TaxonomyConfig {
    /// One `tags { .. }` block, defaulting to the frontmatter key that shares
    /// the taxonomy's id. A bare `tags` groups pages and generates nothing, so
    /// the block is optional.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let id = node.name().value().to_owned();
        let mut taxonomy = Self::from(id.clone());
        Self::line(node, text)?;
        if node.children().is_some() {
            taxonomy.fill(node, text)?;
        }
        taxonomy.check(&id, node, text)?;
        Ok((id, taxonomy))
    }

    /// Refuse a key that only means something beside another, and would
    /// otherwise parse and configure nothing.
    fn check(&self, id: &str, node: &KdlNode, text: &str) -> Result<()> {
        let entities = self.entities.is_some();
        let required = [
            (self.credit.is_some(), "credit", "entities", entities),
            (self.describe, "describe", "entities", entities),
            (self.describe, "describe", "listing", self.listing.enabled),
        ];
        for (written, key, needs, satisfied) in required {
            if !written || satisfied {
                continue;
            }
            return Err(ConfigError::at(
                text,
                ConfigErrorKind::TaxonomyRequires {
                    taxonomy: id.to_owned(),
                    key,
                    needs,
                    help: markup!("write `{}` beside it, or drop `{}`", needs, key),
                },
                NodeExt::span(node),
            )
            .into());
        }
        Ok(())
    }
}
