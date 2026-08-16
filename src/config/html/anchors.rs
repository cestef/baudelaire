//! `html { anchors { } }`: heading ids, and the link back to them.

use crate::config::Named;
use crate::config::dispatch::Kind::{Choice, Numbers, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Where a heading's self link sits relative to the heading's own text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Place {
    #[default]
    After,
    Before,
}

impl Named for Place {
    const NAMES: &'static [(&'static str, Self)] =
        &[("after", Self::After), ("before", Self::Before)];
}

/// Deep-linkable headings: which get an `id`, and whether a reader is given
/// something to click to copy it.
#[derive(Debug, Clone, Hash)]
pub struct AnchorConfig {
    /// Derive an `id` for a heading that has none. An authored `id` is always
    /// left alone.
    pub enabled: bool,
    /// The heading levels that get one, as `1`..`6`. Empty means every level.
    pub levels: Vec<u8>,
    /// The text of the link back to a heading, e.g. `#` or `¶`. `None` emits no
    /// link.
    pub link: Option<String>,
    pub place: Place,
}

impl AnchorConfig {
    /// The class the self link carries, so a stylesheet has one name to reach
    /// it by.
    pub const CLASS: &'static str = "anchor";

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
            levels: Vec::new(),
            link: None,
            place: Place::default(),
        }
    }
}

impl Section for AnchorConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "levels",
            Numbers,
            "The heading levels that get an id, as `1` to `6`. Unset, every level does.",
            |c, n, t| {
                c.levels = n.bounds::<u8>(t, 1, 6)?;
                Ok(())
            },
        ),
        (
            "link",
            Text,
            "The text of a link back to each heading, e.g. `#`. Unset or empty, no link is emitted.",
            |c, n, t| {
                let text = n.string(t, 0)?;
                c.link = (!text.is_empty()).then_some(text);
                Ok(())
            },
        ),
        (
            "place",
            Choice(Place::names),
            "Which side of the heading's text that link sits on.",
            |c, n, t| {
                c.place = n.arg(t, 0)?.one::<Place>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
    ]);
}
