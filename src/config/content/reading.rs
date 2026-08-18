//! `content { reading { } }`: how a page's reading estimate is measured.

use crate::config::dispatch::Kind::Number;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// How long a page takes to read, as the rate `words / wpm` behind
/// `page.reading.minutes`. A language may state its own with
/// `languages { ja { wpm } }`; this answers for the ones that do not.
#[derive(Debug, Clone, Copy, Hash)]
pub struct ReadingConfig {
    pub wpm: usize,
}

impl Default for ReadingConfig {
    fn default() -> Self {
        Self { wpm: 200 }
    }
}

impl Section for ReadingConfig {
    const RULES: Block<Self> = Block(&[(
        "wpm",
        Number,
        "Words a reader gets through in a minute, for `page.reading.minutes`.",
        |c| c.wpm.into(),
        |c, n, t| {
            c.wpm = usize::from(
                n.arg(t, 0)?
                    .bounded::<u16>(t, NodeExt::span(n), 1, u16::MAX)?,
            );
            Ok(())
        },
    )]);
}
