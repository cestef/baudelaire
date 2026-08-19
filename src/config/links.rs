//! `links { }`: the shape of a generated URL, and the link graph.

use crate::config::Value;
use crate::config::dispatch::Kind::{Choice, Flag};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{Named, UrlStyle};

/// What a page's URL looks like, and what the build does with the graph of
/// references to it. What it *verifies* about them is `check { }`.
#[derive(Debug, Clone, Hash, Default)]
pub struct LinkConfig {
    /// How permalinks map onto output files: clean (directory-per-page) or flat
    /// (`.html`).
    pub style: UrlStyle,
    /// Hand each page the pages whose content links to it, as `page.backlinks`.
    /// Opt-in: a page whose backlinks turn out wrong is compiled a second time.
    pub backlinks: bool,
}

impl Section for LinkConfig {
    const RULES: Block<Self> = Block(&[
        (
            "style",
            Choice(UrlStyle::names),
            "Whether URLs are directories (`clean`) or `.html` files (`flat`).",
            |c| Value::named(c.style),
            |c, n, t| {
                c.style = n.arg(t, 0)?.one::<UrlStyle>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "backlinks",
            Flag,
            "Hand each page the pages whose content links to it, as `page.backlinks`.",
            |c| c.backlinks.into(),
            |c, n, t| {
                c.backlinks = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
