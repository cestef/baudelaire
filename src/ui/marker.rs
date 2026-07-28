//! The glyphs the CLI prints in front of a line, and the colour each carries.
//!
//! One table, so a marker means the same thing wherever it appears and no
//! caller hand-rolls a glyph or a colour of its own. Every variant renders
//! already styled: the `anstream` writer behind [`crate::ui::Ui`] strips the
//! colour on pipes and under `NO_COLOR`.

use std::fmt::Display;

use owo_colors::OwoColorize;

/// A status glyph. Rendering is [`Display`], so a marker drops into any format
/// string without allocating a styled `String` first.
#[derive(Debug, Clone, Copy)]
pub enum Marker {
    /// Heads a stage: `◆ standard.site - 24 documents`.
    Section,
    /// A finished operation's result line.
    Done,
    /// An indented sub-item hanging off the line above it.
    Item,
    /// A row of a hanging tree, and the rounded connector its last row uses.
    Branch,
    End,
    /// Points at a value the reader will want (a URL, the watch list).
    Pointer,
    /// Work in progress, on the transient status line.
    Working,
    /// An item deliberately not processed.
    Skipped,
    /// A file changed on disk, in the dev-server event log.
    Changed,
    /// A page that was compiled, served from the cache, or failed.
    Built,
    Cached,
    Failed,
    /// A file sent to, or removed from, a deploy destination. Uncoloured: they
    /// come one per file under a result line that is already coloured.
    Uploaded,
    Removed,
}

impl Display for Marker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Section => write!(f, "{}", "◆".cyan().bold()),
            Self::Done => write!(f, "{}", "✓".green().bold()),
            Self::Item => write!(f, "{}", "↳".dimmed()),
            Self::Branch => write!(f, "{}", "├─".dimmed()),
            Self::End => write!(f, "{}", "╰─".dimmed()),
            Self::Pointer => write!(f, "{}", "➜".green().bold()),
            Self::Working => write!(f, "{}", "⟳".cyan()),
            Self::Skipped => write!(f, "{}", "·".dimmed()),
            Self::Changed => write!(f, "{}", "~".green()),
            Self::Built => write!(f, "{}", "✓".green()),
            Self::Cached => write!(f, "{}", "→".cyan()),
            Self::Failed => write!(f, "{}", "✗".red()),
            Self::Uploaded => f.write_str("↑"),
            Self::Removed => f.write_str("✕"),
        }
    }
}

/// Per-page build status for progress reporting: its marker plus the word that
/// follows the path.
#[derive(Debug, Clone, Copy)]
pub enum PageStatus {
    Built,
    Cached,
    Failed,
}

impl PageStatus {
    /// The glyph this status prints with.
    pub(super) fn marker(self) -> Marker {
        match self {
            Self::Built => Marker::Built,
            Self::Cached => Marker::Cached,
            Self::Failed => Marker::Failed,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Built => "built",
            Self::Cached => "cached",
            Self::Failed => "failed",
        }
    }
}
