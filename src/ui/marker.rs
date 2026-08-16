//! The glyphs the CLI prints in front of a line, and the colour each carries.

use std::fmt::Display;

use owo_colors::OwoColorize;

/// A status glyph, rendered already styled.
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
    /// Points at a value the reader will want, such as a URL.
    Pointer,
    /// Work in progress, on the transient status line.
    Working,
    Skipped,
    /// A file changed on disk, in the dev-server event log.
    Changed,
    Built,
    Cached,
    Failed,
    /// A file sent to, or removed from, a deploy destination; uncoloured
    /// because it hangs under a result line that already carries the colour.
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

/// Per-page build status, as the progress output reports it.
#[derive(Debug, Clone, Copy)]
pub enum PageStatus {
    Built,
    Cached,
    Failed,
}

impl PageStatus {
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
