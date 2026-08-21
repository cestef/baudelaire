//! `check { headings { } }`: the heading-level rule and the outline it judges
//! against.

use dispatch_derive::Table;

use crate::config::Level;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// How loud a skipped heading level is, and the level a page's own outline
/// opens at.
#[derive(Debug, Clone, Default, Hash, Table)]
pub struct HeadingConfig {
    /// How loud a skipped level is.
    #[key(level)]
    pub level: Level,

    /// The level a page's own sections open at, as `1` to `6`. The first heading under the layout's own may land there without counting as a skip.
    ///
    /// `None` holds a page to one level at a time from the top.
    #[key(opt bounded(u8, 1, 6))]
    pub start: Option<u8>,
}

impl HeadingConfig {
    /// Whether `level` is the one a page's outline opens at, and so may follow
    /// the heading a template wrote without counting as a skip.
    pub fn opens(&self, level: u8) -> bool {
        self.start == Some(level)
    }
}
