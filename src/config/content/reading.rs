//! `content { reading { } }`: how a page's reading estimate is measured.

use crate::config::dispatch::Kind::Number;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// How long a page takes to read, per word.
///
/// A rate, not a duration: the estimate is `words / wpm`, and the estimate is
/// what a template shows as `page.reading.minutes`.
///
/// Configurable because the figure is language-shaped and was a constant. 200 is
/// prose in a European language; Japanese and Chinese are read three times
/// faster by *word*, so a site in one reported every article as a third of the
/// read it is. A language may state its own with `languages { ja { wpm } }`, and
/// this is the site's answer for the ones that do not.
#[derive(Debug, Clone, Copy, Hash)]
pub struct ReadingConfig {
    /// Words a reader gets through in a minute.
    pub wpm: usize,
}

impl Default for ReadingConfig {
    fn default() -> Self {
        // The figure every other generator uses for prose, and the one a reader
        // has been calibrated against by every "6 min read" badge they have seen.
        Self { wpm: 200 }
    }
}

impl Section for ReadingConfig {
    const RULES: Block<Self> = Block(&[(
        "wpm",
        Number,
        "Words a reader gets through in a minute, for `page.reading.minutes`.",
        |c, n, t| {
            // A rate of nothing is a division by zero dressed as a setting.
            c.wpm = usize::from(
                n.arg(t, 0)?
                    .bounded::<u16>(t, NodeExt::span(n), 1, u16::MAX)?,
            );
            Ok(())
        },
    )]);
}
