//! `content { taxonomies { } }`: a term key, the pages it groups, and the
//! listings it generates over them.

use kdl::KdlNode;

use crate::config::Value;
use crate::config::dispatch::Kind::{Block as Nested, Choice, Flag, Number, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{Named, PaginateConfig, SortKey};
use crate::content::Credit;
use crate::error::{ConfigError, ConfigErrorKind, Result};
use crate::ui::markup;

#[derive(Debug, Clone, Hash)]
pub struct TaxonomyConfig {
    /// Frontmatter key to read terms from.
    pub key: String,
    /// What a page claims about the entities it names here: `authors` credits
    /// them with writing it. Only meaningful with `entities`.
    pub credit: Option<Credit>,
    /// The `content { entities { } }` registry this taxonomy's terms are ids
    /// in. `None` is a plain taxonomy, whose terms are words.
    pub entities: Option<String>,
    /// The page generated per term, and the index every term appears on.
    pub listing: ListingConfig,
    /// Let a term that names an entity written as a *page* be described by that
    /// page, instead of generating a listing of its own.
    pub describe: bool,
    /// What a term's members are ordered by.
    pub sort: SortKey,
    /// Reverse that order.
    pub reverse: bool,
}

/// A taxonomy's generated listings: whether there are any, how a term page is
/// chunked, and what renders it.
#[derive(Debug, Clone, Hash)]
pub struct ListingConfig {
    /// Whether the term pages and their index are generated at all: the
    /// block's presence.
    pub enabled: bool,
    /// Members per term page. `None` puts every member on one page.
    pub size: Option<usize>,
    /// Template for the generated term pages and their index.
    pub template: Option<String>,
    /// Path segment before a term page's number (`/tags/rust/page/2/`); empty
    /// drops it.
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

impl Section for TaxonomyConfig {
    const RULES: Block<Self> = Block(&[
        (
            "key",
            Text,
            "The frontmatter field its terms are read from. Defaults to the taxonomy's own id.",
            |c| c.key.clone().into(),
            |c, n, t| {
                c.key = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "entities",
            Text,
            "The `content { entities { } }` registry its terms are ids in.",
            |c| c.entities.clone().into(),
            |c, n, t| {
                c.entities = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "credit",
            Choice(Credit::names),
            "What a page claims about the entities it names here, for the surfaces that can spell it.",
            |c| c.credit.map(Value::named).into(),
            |c, n, t| {
                c.credit = Some(n.arg(t, 0)?.one::<Credit>(t, NodeExt::span(n))?);
                Ok(())
            },
        ),
        (
            "listing",
            Nested(ListingConfig::rows),
            "Generate a page per term, and an index of the terms. Its presence turns them on; `#false` turns them off again.",
            |c| c.listing.values(),
            |c, n, t| c.listing.fill(n, t),
        ),
        (
            "describe",
            Flag,
            "Let a term written as a profile page be described by it, instead of generating a listing beside it.",
            |c| c.describe.into(),
            |c, n, t| {
                c.describe = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "sort",
            Choice(SortKey::names),
            "What a term's members are ordered by. Defaults to `title`, since a term spans collections.",
            |c| Value::named(c.sort),
            |c, n, t| {
                c.sort = n.arg(t, 0)?.one::<SortKey>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "reverse",
            Flag,
            "Reverse that order, for the newest-first a dated term listing wants.",
            |c| c.reverse.into(),
            |c, n, t| {
                c.reverse = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}

impl Section for ListingConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "size",
            Number,
            "Members per term page. Omitted, a term's listing is one page.",
            |c| c.size.into(),
            |c, n, t| {
                let written = n.arg(t, 0)?.integer(t, NodeExt::span(n))?;
                c.size = Some(PaginateConfig::size(written, t, NodeExt::span(n))?);
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The layout those listings render through.",
            |c| c.template.clone().into(),
            |c, n, t| {
                c.template = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "prefix",
            Text,
            "The path segment before a term page's number, as in `/tags/rust/page/2/`.",
            |c| c.prefix.clone().into(),
            |c, n, t| {
                c.prefix = n.template(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
