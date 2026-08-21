//! `content { collections { } }`: a collection and its generated index.

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::Value;
use crate::config::dispatch::Kind::{Items, Number};
use crate::config::dispatch::{Attributed, Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::vocab::rule;
use crate::config::{FieldSchema, Named, Permalink};
use crate::error::{ConfigError, Result};

#[derive(Debug, Clone, Hash, Table)]
#[table(items {
    /// The glob, read off the line by [`CollectionConfig::item`].
    const LEADING: usize = 1;
})]
pub struct CollectionConfig {
    /// Which content files belong to this collection.
    ///
    /// `None` is convention: a top-level directory under `content/`.
    #[key(opt text)]
    pub glob: Option<String>,

    /// What the collection's members are ordered by.
    #[key(choice(SortKey))]
    pub sort: SortKey,

    /// Reverse that order.
    #[key(flag)]
    pub reverse: bool,

    /// The URL pattern its pages publish at, e.g. `/{slug}/`.
    #[key(opt template)]
    pub permalink: Option<String>,

    /// The layout its pages render through.
    #[key(opt text)]
    pub template: Option<String>,

    /// Generate an index over the collection. Its presence turns the index on; `#false` turns it off again.
    #[key(nested(PaginateConfig))]
    pub paginate: PaginateConfig,

    /// Also write a feed of this collection's members, beside its index.
    ///
    /// In every configured format.
    #[key(flag)]
    pub feed: bool,

    /// What every member's frontmatter must declare, one line per field. A `dict` field takes a block of its own fields.
    ///
    /// In declaration order. Empty requires nothing.
    #[key(custom(
        Items(FieldSchema::rows),
        |c: &Self| Value::each(&c.schema, Attributed::values),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.schema = n.unique(t, "schema field", FieldSchema::item)?;
            Ok(())
        },
    ))]
    pub schema: Vec<(String, FieldSchema)>,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            glob: None,
            sort: SortKey::Order,
            reverse: false,
            permalink: None,
            template: None,
            paginate: PaginateConfig::default(),
            feed: false,
            schema: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SortKey {
    /// Frontmatter `order` field, ascending.
    #[default]
    Order,
    /// Frontmatter `date` field, ascending.
    Date,
    /// Frontmatter `title` field, alphabetical.
    Title,
}

impl Named for SortKey {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("order", Self::Order),
        ("date", Self::Date),
        ("title", Self::Title),
    ];
}

/// A collection's generated index: whether there is one, how it is chunked, and
/// where it is served.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct PaginateConfig {
    /// Whether an index is generated at all: the block's presence.
    pub enabled: bool,

    /// Pages per index page. Omitted, the index is one page.
    #[key(custom(
        Number,
        |c: &Self| c.size.into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let written = n.arg(t, 0)?.integer(t, NodeExt::span(n))?;
            c.size = Some(Self::size(written, t, NodeExt::span(n))?);
            Ok(())
        },
    ))]
    pub size: Option<usize>,

    /// The layout the index renders through.
    ///
    /// As distinct from the collection's `template`, which wraps its members.
    #[key(opt text)]
    pub template: Option<String>,

    /// Where the index publishes, if not at the collection's own path.
    ///
    /// `None` is `/{id}/`.
    #[key(opt segment)]
    pub mount: Option<String>,

    /// The path segment before a page number, as in `/posts/page/2/`.
    ///
    /// `/{id}/{prefix}/{n}/`. Defaults to `page`; empty drops the segment
    /// (`/blog/2/`).
    #[key(segment)]
    pub prefix: String,
}

impl CollectionConfig {
    /// Where this collection's index sits, before localization: the
    /// `paginate { mount }` if one moves it, else `/{id}/`. Always rooted, or
    /// an unrooted `mount "blog"` localizes to `/frblog`.
    pub fn home(&self, id: &str) -> String {
        match self.paginate.mount.as_deref() {
            Some(mount) if mount.starts_with('/') => mount.to_owned(),
            Some(mount) => format!("/{mount}"),
            None => Permalink::join(&[id]),
        }
    }
}

impl PaginateConfig {
    /// The path segment before a page number when nothing names one.
    pub(crate) const PREFIX: &'static str = "page";

    /// A written `paginate` count as a page size, shared with the taxonomy
    /// listings, which paginate by the same rules under a different key.
    pub(crate) fn size(n: i64, text: &str, span: miette::SourceSpan) -> Result<usize> {
        if n < 1 {
            return Err(ConfigError::paginate_too_small(text, n, span).into());
        }
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }
}

impl Default for PaginateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            size: None,
            template: None,
            mount: None,
            prefix: Self::PREFIX.into(),
        }
    }
}

impl CollectionConfig {
    /// One `posts { .. }` block: the node name is the collection id, and an
    /// optional leading positional its member glob.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let mut cfg = Self::default();
        Self::line(node, text)?;
        if let Some(glob) = node.get(0_usize) {
            cfg.glob = Some(glob.as_str(text, NodeExt::span(node))?);
        }
        if node.children().is_some() {
            cfg.fill(node, text)?;
        }
        Ok((node.name().value().to_owned(), cfg))
    }
}
