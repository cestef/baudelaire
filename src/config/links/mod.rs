//! `links { }`: link shape, link checking, and the link graph.

pub mod external;

use crate::config::dispatch::Kind::{Block as Nested, Choice, Flag};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{ExternalConfig, Named, UrlStyle};

/// Link shape and link checking: what a page's URL looks like, and how hard the
/// build tries to prove every reference to one resolves.
#[derive(Debug, Clone, Hash)]
pub struct LinkConfig {
    /// How permalinks map onto output files: clean (directory-per-page) or flat
    /// (`.html`).
    pub style: UrlStyle,
    /// Treat unresolved internal `.typ` links as errors (else warnings).
    pub strict: bool,
    pub external: ExternalConfig,
    /// Hand each page the pages whose content links to it, as `page.backlinks`.
    /// Opt-in: a page whose backlinks turn out wrong is compiled a second time.
    pub backlinks: bool,
    /// Report the pages nothing links to, and what counts as a link. `None`
    /// leaves the report off.
    pub orphans: Option<Linked>,
}

/// What counts as pointing at a page, for the orphan report. A layout's own
/// links never count under either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Linked {
    /// Any page's link, a generated index or term page included.
    #[default]
    Any,
    /// Only a link on a page an author wrote.
    Authored,
}

impl Named for Linked {
    const NAMES: &'static [(&'static str, Self)] =
        &[("any", Self::Any), ("authored", Self::Authored)];
}

impl Linked {
    /// Whether a link on this page counts. `generated` is whether the build
    /// wrote the page rather than an author.
    pub fn counts(self, generated: bool) -> bool {
        !generated || self == Self::Any
    }
}

impl LinkConfig {
    /// Whether this build needs the site's link graph at all; the one gate the
    /// render pass records edges behind.
    pub fn graph(&self) -> bool {
        self.backlinks || self.orphans.is_some()
    }
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            style: UrlStyle::default(),
            strict: true,
            external: ExternalConfig::default(),
            backlinks: false,
            orphans: None,
        }
    }
}

impl Section for LinkConfig {
    const RULES: Block<Self> = Block(&[
        (
            "style",
            Choice(UrlStyle::names),
            "Whether URLs are directories (`clean`) or `.html` files (`flat`).",
            |c, n, t| {
                c.style = n.arg(t, 0)?.one::<UrlStyle>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "strict",
            Flag,
            "Fail the build on a broken internal link instead of warning.",
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "external",
            Nested(ExternalConfig::rows),
            "Check outbound `http(s)` links over the network. Its presence turns it on; `#false` turns it off again.",
            |c, n, t| c.external.fill(n, t),
        ),
        (
            "backlinks",
            Flag,
            "Hand each page the pages whose content links to it, as `page.backlinks`.",
            |c, n, t| {
                c.backlinks = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "orphans",
            Choice(Linked::names),
            "Report the pages nothing links to, counting `any` page's links or only those an author wrote.",
            |c, n, t| {
                c.orphans = Some(n.arg(t, 0)?.one::<Linked>(t, NodeExt::span(n))?);
                Ok(())
            },
        ),
    ]);
}
