//! `lint { headings { } }`: the heading-level rule and the outline it judges
//! against.

use crate::config::dispatch::Kind::{Level as Loud, Number};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{Level, Named, Severity};

/// How loud a skipped heading level is, and the level a page's own outline
/// opens at.
#[derive(Debug, Clone, Default, Hash)]
pub struct HeadingConfig {
    pub level: Level,
    /// The level a page's sections open at, where the template writes the `h1`
    /// itself: the heading right after that one may land here without counting
    /// as a skip. `None` holds a page to one level at a time from the top.
    pub start: Option<u8>,
}

impl Section for HeadingConfig {
    const RULES: Block<Self> = Block(&[
        (
            "level",
            Loud(Severity::names),
            "How loud a skipped level is.",
            |c| c.level.into(),
            |c, n, t| {
                c.level = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            "start",
            Number,
            "The level a page's own sections open at, as `1` to `6`. The first heading under the layout's own may land there without counting as a skip.",
            |c| c.start.into(),
            |c, n, t| {
                c.start = Some(n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, 6)?);
                Ok(())
            },
        ),
    ]);
}

impl HeadingConfig {
    /// Whether `level` is the one a page's outline opens at, and so may follow
    /// the heading a template wrote without counting as a skip.
    pub fn opens(&self, level: u8) -> bool {
        self.start == Some(level)
    }
}
