//! `links { }`: the shape of a generated URL, and the link graph.

use dispatch_derive::Table;

use crate::config::UrlStyle;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// What a page's URL looks like, and what the build does with the graph of
/// references to it. What it *verifies* about them is `check { }`.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct LinkConfig {
    /// Whether URLs are directories (`clean`) or `.html` files (`flat`).
    ///
    /// How permalinks map onto output files.
    #[key(choice(UrlStyle))]
    pub style: UrlStyle,

    /// Hand each page the pages whose content links to it, as `page.backlinks`.
    ///
    /// Opt-in: a page whose backlinks turn out wrong is compiled a second time.
    #[key(flag)]
    pub backlinks: bool,
}
