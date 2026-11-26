//! `html { anchors { } }`: heading ids, and the link back to them.

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;

/// Where a heading's self link sits relative to the heading's own text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Place {
    #[default]
    After,
    Before,
    /// The heading's own text is the link, and `link` is not used.
    Heading,
}

impl Named for Place {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("after", Self::After),
        ("before", Self::Before),
        ("heading", Self::Heading),
    ];
}

/// Deep-linkable headings: which get an `id`, and whether a reader is given
/// something to click to copy it.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct AnchorConfig {
    /// Derive an `id` for a heading that has none. An authored `id` is always
    /// left alone.
    pub enabled: bool,

    /// Give every element carrying a Typst label that label as its `id`, so
    /// `page.typ#label` lands on it. A labelled heading takes the label over its slug.
    #[key(flag)]
    pub labels: bool,

    /// The heading levels that get an id, as `1` to `6`. Unset, every level does.
    #[key(numbers(u8, 1, 6))]
    pub levels: Vec<u8>,

    /// The text of a link back to each heading, e.g. `§`. Unset, a `place` of `before` or `after` reads `#`; empty, no link is emitted.
    #[key(custom(
        Text,
        |c: &Self| c.link.clone().into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.link = Some(n.string(t, 0)?);
            Ok(())
        },
    ))]
    pub link: Option<String>,

    /// Which side of the heading's text that link sits on, or `heading` to make the text itself the link.
    #[key(opt choice(Place))]
    pub place: Option<Place>,
}

impl AnchorConfig {
    /// The class the self link carries, so a stylesheet has one name to reach
    /// it by.
    pub const CLASS: &'static str = "anchor";

    /// The text a link placed beside its heading reads when `link` is unset.
    pub const GLYPH: &'static str = "#";

    /// The text of the link beside each heading, or `None` when there is none:
    /// none was asked for, an empty `link` took it back, or the heading is its
    /// own link.
    pub fn beside(&self) -> Option<&str> {
        match (self.place, self.link.as_deref()) {
            (Some(Place::Heading), _) | (_, Some("")) | (None, None) => None,
            (_, Some(text)) => Some(text),
            (Some(_), None) => Some(Self::GLYPH),
        }
    }

    /// Whether a heading at `level` gets an id; an empty `levels` narrows
    /// nothing and so covers every level.
    pub fn covers(&self, level: u8) -> bool {
        self.levels.is_empty() || self.levels.contains(&level)
    }
}

impl Default for AnchorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            labels: true,
            levels: Vec::new(),
            link: None,
            place: None,
        }
    }
}
