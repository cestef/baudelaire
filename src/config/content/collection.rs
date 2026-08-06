//! `content { collections { } }`: a collection and its generated index.

use kdl::KdlNode;

use crate::config::dispatch::Kind::{Block as Nested, Choice, Flag, Items, Number, Template, Text};
use crate::config::dispatch::{Attributed, Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{FieldSchema, Named, Permalink};
use crate::error::{ConfigError, Result};

/// Per-collection override.
#[derive(Debug, Clone, Hash)]
pub struct CollectionConfig {
    /// Glob selecting members. `None` = convention (top-level dir under `content/`).
    pub glob: Option<String>,
    /// Sort key.
    pub sort: SortKey,
    /// Reverse sort order.
    pub reverse: bool,
    /// Permalink template, e.g. `/posts/{slug}/`.
    pub permalink: Option<String>,
    /// Default template file for pages in this collection.
    pub template: Option<String>,
    /// The generated index over this collection's members.
    pub paginate: PaginateConfig,
    /// Also write a feed of this collection's members, beside its index, in
    /// every configured format.
    ///
    /// The site feed carries everything dated, which is the wrong granularity
    /// for a site that publishes more than one kind of thing: a reader who
    /// wants the essays has to take the release notes too. Per collection
    /// rather than a blanket flag, because on any site with a `docs` or a
    /// `pages` collection most of them want no feed at all.
    pub feed: bool,
    /// What every member's frontmatter must declare, in declaration order.
    /// Empty is the default: nothing required, nothing typed.
    pub schema: Vec<(String, FieldSchema)>,
}

impl CollectionConfig {
    /// Where this collection's index sits, before localization: the
    /// `paginate { mount }` if one moves it, else `/{id}/`, always rooted.
    ///
    /// The rooting is not cosmetic: an unrooted `mount "blog"` localizes to
    /// `/frblog` rather than `/fr/blog`, and this now names an output file as
    /// well as a permalink.
    ///
    /// The single answer, so the index page, the feed written beside it and the
    /// autodiscovery tag pointing at that feed cannot disagree about where the
    /// collection lives. [`crate::content::Pagination`] takes it as page 1's
    /// permalink rather than deriving the same rule again.
    pub fn home(&self, id: &str) -> String {
        match self.paginate.mount.as_deref() {
            Some(mount) if mount.starts_with('/') => mount.to_owned(),
            Some(mount) => format!("/{mount}"),
            None => Permalink::join(&[id]),
        }
    }
}

/// A collection's generated index: whether there is one, how it is chunked, and
/// where it is served.
///
/// One block, because it is one concept. It used to be four flat attributes
/// sitting beside the ones that shape *member* pages, and three of the four
/// names did not say what they meant: `list` was a template rather than a list,
/// `mount` the permalink of page 1, `prefix` the segment before a page number.
/// Reading `template` next to `list` gave no clue that the first wrapped a post
/// and the second the index over them.
#[derive(Debug, Clone, Hash)]
pub struct PaginateConfig {
    /// Whether an index is generated at all: the block's presence.
    pub enabled: bool,
    /// Members per page. `None` puts every member on one page, which is what a
    /// listing with no size is.
    pub size: Option<usize>,
    /// Template for the generated index pages, as distinct from the collection's
    /// `template`, which wraps its members.
    pub template: Option<String>,
    /// Where page 1 is served. `None` = `/{id}/`; set to `/` to mount a blog at
    /// the site root.
    pub mount: Option<String>,
    /// Path segment before a page number: `/{id}/{prefix}/{n}/`. Defaults to
    /// `page` (`/blog/page/2/`); empty drops the segment (`/blog/2/`).
    pub prefix: String,
}

/// Ordering key for a collection's pages.
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
            // no schema is the default: a collection constrains its members'
            // frontmatter only when it says so.
            schema: Vec::new(),
        }
    }
}

impl Default for PaginateConfig {
    fn default() -> Self {
        Self {
            // opt-in: the presence of a `paginate { .. }` block flips it, and a
            // collection with no index generates none.
            enabled: false,
            // no size is one page holding every member
            size: None,
            template: None,
            mount: None,
            prefix: "page".into(),
        }
    }
}

impl CollectionConfig {
    /// One `posts { .. }` block: the node name is the collection id, and an
    /// optional leading positional its member glob, so the one-line
    /// `posts "posts/**/*.typ"` shorthand still reads.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let mut cfg = Self::default();
        // At most the glob, and never a `key=value`: a collection's keys are
        // child nodes, so `posts sort="date"` used to be read as the *glob*,
        // silently, with the sort it asked for dropped. Checked here rather
        // than in `fill` alone, because a line with no block never reaches it.
        Self::line(node, text)?;
        if let Some(glob) = node.get(0_usize) {
            cfg.glob = Some(glob.as_str(text, NodeExt::span(node))?);
        }
        // A bare `posts` or `posts "glob"` is a collection that only declares
        // its members; everything else it could say lives in the block.
        if node.children().is_some() {
            cfg.fill(node, text)?;
        }
        Ok((node.name().value().to_owned(), cfg))
    }
}

/// One collection: what belongs to it, how its members are ordered and
/// addressed, and (in a nested block) the index generated over them.
impl Section for CollectionConfig {
    const RULES: Block<Self> = Block(&[
        (
            "glob",
            Text,
            "Which content files belong to this collection.",
            |c, n, t| {
                c.glob = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "sort",
            Choice(SortKey::names),
            "What the collection's members are ordered by.",
            |c, n, t| {
                c.sort = n.arg(t, 0)?.one::<SortKey>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        ("reverse", Flag, "Reverse that order.", |c, n, t| {
            c.reverse = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "permalink",
            Template,
            "The URL pattern its pages publish at, e.g. `/{slug}/`.",
            |c, n, t| {
                // validated here so a template typo is a spanned config error,
                // not a silent fallback to convention at page load
                c.permalink = Some(n.template(t, 0)?);
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The layout its pages render through.",
            |c, n, t| {
                c.template = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "paginate",
            Nested(PaginateConfig::rows),
            "Generate an index over the collection. Its presence turns the index on; `#false` turns it off again.",
            |c, n, t| c.paginate.fill(n, t),
        ),
        (
            "feed",
            Flag,
            "Also write a feed of this collection's members, beside its index.",
            |c, n, t| {
                c.feed = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "schema",
            Items(FieldSchema::rows),
            "What every member's frontmatter must declare, one line per field. A `dict` field takes a block of its own fields.",
            |c, n, t| {
                c.schema = n.unique(t, "schema field", FieldSchema::item)?;
                Ok(())
            },
        ),
    ]);

    /// The glob, which [`CollectionConfig::item`] reads off the line before the
    /// block is dispatched.
    const LEADING: usize = 1;
}

/// The `paginate { .. }` block inside a collection: its presence generates the
/// index, and every key tunes it.
impl Section for PaginateConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "size",
            Number,
            "Pages per index page. Omitted, the index is one page.",
            |c, n, t| {
                let n_ = n.arg(t, 0)?.integer(t, NodeExt::span(n))?;
                if n_ < 1 {
                    return Err(ConfigError::paginate_too_small(t, n_, NodeExt::span(n)).into());
                }
                // `n_` is proved positive above; a page size wider than `usize`
                // (only reachable on a 32-bit target) still means "one page".
                c.size = Some(usize::try_from(n_).unwrap_or(usize::MAX));
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The layout the index renders through.",
            |c, n, t| {
                c.template = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "mount",
            Text,
            "Where the index publishes, if not at the collection's own path.",
            |c, n, t| {
                // Both of these are pieces of the index's permalink, so both go
                // through the template parser for its `..` rule, exactly as
                // `permalink` does. `Config::segments` drops a `..` before
                // anything is written, so the damage was never an escaped file:
                // it was an index published at a URL nobody asked for, out of a
                // green build.
                c.mount = Some(n.template(t, 0)?);
                Ok(())
            },
        ),
        (
            "prefix",
            Text,
            "The path segment before a page number, as in `/posts/page/2/`.",
            |c, n, t| {
                c.prefix = n.template(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
