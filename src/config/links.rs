//! `links { }`: link shape, link checking, and the link graph.

use crate::config::dispatch::Kind::{Choice, Flag};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{Named, UrlStyle};

/// Link shape and link checking: what a page's URL looks like, and how hard the
/// build tries to prove every reference to one resolves.
#[derive(Debug, Clone, Hash)]
pub struct LinkConfig {
    /// How permalinks map onto output files: clean (directory-per-page) or flat
    /// (`.html`). Set under `links { style "clean" | "flat" }`.
    pub style: UrlStyle,
    /// Treat unresolved internal `.typ` links as errors (else warnings).
    pub strict: bool,
    /// Also verify outbound `http(s)` links over the network.
    ///
    /// Read by `check` alone: a build stays offline and deterministic, so a
    /// flaky host or an airplane can never change what it produces. `check
    /// --external` turns it on for one run.
    pub external: bool,
    /// Hand each page the pages whose content links to it, as `page.backlinks`.
    ///
    /// Opt-in because it is the one page value that cannot be known before the
    /// site has rendered: a page whose backlinks turn out wrong is compiled a
    /// second time (see `engine::links::Graph`), which a site that shows none
    /// should not pay for.
    pub backlinks: bool,
    /// Report the pages nothing links to, and what counts as a link. `None`
    /// leaves the report off.
    pub orphans: Option<Linked>,
}

/// What counts as pointing at a page, for the orphan report.
///
/// A layout never does under either: a sidebar links every page from every page,
/// so counting one would mean no page is ever an orphan. The difference is
/// whether a page the *build* generated counts as a reader's way in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Linked {
    /// Any page's link. A post reached from its paginated index or from a term
    /// page is reached, so the report names only what a reader cannot get to at
    /// all.
    #[default]
    Any,
    /// Only a link on a page an author wrote. A post reached from its index and
    /// from nowhere else is named, which is the question a documentation site
    /// asks: did anyone write about this page?
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
    /// Whether this build needs the site's link graph at all.
    ///
    /// The one gate the render pass records edges behind, and the one both
    /// readers of them share: a page's backlinks and the orphan report are the
    /// same graph asked two questions. Without it nothing walks a link's origin
    /// and no page carries the edges in its cache entry.
    pub fn graph(&self) -> bool {
        self.backlinks || self.orphans.is_some()
    }
}

impl Default for LinkConfig {
    fn default() -> Self {
        // Strict internal links by default: a `.typ` link naming no page is a
        // typo, and the build knows it for certain. External checking is opt-in
        // and needs the network, so it can never be a default.
        Self {
            style: UrlStyle::default(),
            strict: true,
            external: false,
            // Off: a page whose backlinks change is compiled twice, which a
            // site that shows none must not pay for.
            backlinks: false,
            // Off: a page reachable only from a hand-written nav is a normal
            // thing to have, so this is a question a site asks, not one it is
            // asked on every build.
            orphans: None,
        }
    }
}

/// The `links { .. }` section: URL shape and link checking.
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
            Flag,
            "Also check outbound `http(s)` links over the network.",
            |c, n, t| {
                c.external = n.boolean(t, 0)?;
                Ok(())
            },
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
