//! `content { taxonomies { } }`: a term key and the pages it groups.

use kdl::KdlNode;

use crate::config::Value;
use crate::config::dispatch::Kind::{Choice, Flag, Number, Text};
use crate::config::dispatch::{Attributed, Attrs};
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
    /// Generate a page per term, plus one listing every term appears on.
    pub listing: bool,
    /// Let a term that names an entity written as a *page* be described by that
    /// page, instead of generating a listing of its own.
    pub describe: bool,
    /// Template for the generated taxonomy index + term pages.
    pub template: Option<String>,
    /// Members per term page. `None` puts every member on one page.
    pub paginate: Option<usize>,
    /// Path segment before a term page's number (`/tags/rust/page/2/`); empty
    /// drops it.
    pub prefix: String,
    /// What a term's members are ordered by.
    pub sort: SortKey,
    /// Reverse that order.
    pub reverse: bool,
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
            listing: false,
            describe: false,
            template: None,
            paginate: None,
            prefix: PaginateConfig::PREFIX.into(),
            sort: SortKey::Title,
            reverse: false,
        }
    }
}

impl TaxonomyConfig {
    /// One `tags key=.. listing=..` line, defaulting to the frontmatter key
    /// that shares the taxonomy's id.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let id = node.name().value().to_owned();
        let mut taxonomy = Self::from(id.clone());
        taxonomy.read(node, text)?;
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
            (self.describe, "describe", "listing", self.listing),
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
                    help: markup!("write `{} {}=..` beside it, or drop `{}`", id, needs, key),
                },
                NodeExt::span(node),
            )
            .into());
        }
        Ok(())
    }
}

impl Attributed for TaxonomyConfig {
    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "key",
            Text,
            "The frontmatter field its terms are read from. Defaults to the taxonomy's own id.",
            |c| c.key.clone().into(),
            |c, v, t, s| {
                c.key = v.as_str(t, s)?;
                Ok(())
            },
        ),
        (
            "entities",
            Text,
            "The `content { entities { } }` registry its terms are ids in.",
            |c| c.entities.clone().into(),
            |c, v, t, s| {
                c.entities = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
        (
            "credit",
            Choice(Credit::names),
            "What a page claims about the entities it names here, for the surfaces that can spell it.",
            |c| c.credit.map(Value::named).into(),
            |c, v, t, s| {
                c.credit = Some(v.one::<Credit>(t, s)?);
                Ok(())
            },
        ),
        (
            "listing",
            Flag,
            "Generate a page per term, and an index of the terms.",
            |c| c.listing.into(),
            |c, v, t, s| {
                c.listing = v.boolean(t, s)?;
                Ok(())
            },
        ),
        (
            "describe",
            Flag,
            "Let a term written as a profile page be described by it, instead of generating a listing beside it.",
            |c| c.describe.into(),
            |c, v, t, s| {
                c.describe = v.boolean(t, s)?;
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The layout those listings render through.",
            |c| c.template.clone().into(),
            |c, v, t, s| {
                c.template = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
        (
            "paginate",
            Number,
            "Pages per term listing.",
            |c| c.paginate.into(),
            |c, v, t, s| {
                c.paginate = Some(PaginateConfig::size(v.integer(t, s)?, t, s)?);
                Ok(())
            },
        ),
        (
            "sort",
            Choice(SortKey::names),
            "What a term's members are ordered by. Defaults to `title`, since a term spans collections.",
            |c| Value::named(c.sort),
            |c, v, t, s| {
                c.sort = v.one::<SortKey>(t, s)?;
                Ok(())
            },
        ),
        (
            "reverse",
            Flag,
            "Reverse that order, for the newest-first a dated term listing wants.",
            |c| c.reverse.into(),
            |c, v, t, s| {
                c.reverse = v.boolean(t, s)?;
                Ok(())
            },
        ),
        (
            "prefix",
            Text,
            "The path segment before a term page's number, as in `/tags/rust/page/2/`.",
            |c| c.prefix.clone().into(),
            |c, v, t, s| {
                c.prefix = v.template(t, s)?;
                Ok(())
            },
        ),
    ]);
}
