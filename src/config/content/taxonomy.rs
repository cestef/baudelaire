//! `content { taxonomies { } }`: a term key and the pages it groups.

use kdl::KdlNode;

use crate::config::dispatch::Kind::{Choice, Flag, Number, Text};
use crate::config::dispatch::{Attributed, Attrs};
use crate::config::value::ValueExt;
use crate::config::{Named, SortKey};
use crate::error::{ConfigError, Result};

/// Taxonomy definition.
#[derive(Debug, Clone, Hash)]
pub struct TaxonomyConfig {
    /// Frontmatter key to read terms from.
    pub key: String,
    /// Generate a page per term, plus one listing every term appears on.
    pub listing: bool,
    /// Template for the generated taxonomy index + term pages.
    pub template: Option<String>,
    /// Members per term page. `None` puts every member on one page, which is
    /// what a term listing used to do unconditionally, beside a collection
    /// index that paginated the same pages.
    pub paginate: Option<usize>,
    /// Path segment before a term page's number (`/tags/rust/page/2/`); empty
    /// drops it. Spelled like a collection's, since it is the same thing.
    pub prefix: String,
    /// What a term's members are ordered by.
    ///
    /// Spelled like a collection's, and read by the same comparator: a term page
    /// used to sort by title unconditionally, so the same posts came in two
    /// orders on one site depending on which listing a reader arrived at.
    pub sort: SortKey,
    /// Reverse that order, which is what a dated term listing wants: newest
    /// first, as a blog index has.
    pub reverse: bool,
}

/// A taxonomy reads the frontmatter key that shares its id unless it names
/// another, so its defaults depend on that id: the conversion *is* the
/// `Default` impl it cannot have.
impl From<String> for TaxonomyConfig {
    fn from(id: String) -> Self {
        Self {
            key: id,
            // opt-in: term pages and their index are extra output
            listing: false,
            template: None,
            // un-paginated until asked, like a collection with no `paginate`
            paginate: None,
            prefix: "page".into(),
            // Title, and not the collection default: a term spans collections,
            // so `order` (a number each collection assigns for itself) orders
            // one term's members against numbers that mean different things.
            // This is also the order term listings have always come in.
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
            "sort",
            Choice(SortKey::names),
            "What a term's members are ordered by. Defaults to `title`, since a term spans collections.",
            |c, v, t, s| {
                c.sort = v.one::<SortKey>(t, s)?;
                Ok(())
            },
        ),
        (
            "reverse",
            Flag,
            "Reverse that order, for the newest-first a dated term listing wants.",
            |c, v, t, s| {
                c.reverse = v.boolean(t, s)?;
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
