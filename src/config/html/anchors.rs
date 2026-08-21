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
}

impl Named for Place {
    const NAMES: &'static [(&'static str, Self)] =
        &[("after", Self::After), ("before", Self::Before)];
}

/// Deep-linkable headings: which get an `id`, and whether a reader is given
/// something to click to copy it.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct AnchorConfig {
    /// Derive an `id` for a heading that has none. An authored `id` is always
    /// left alone.
    pub enabled: bool,

    /// The heading levels that get an id, as `1` to `6`. Unset, every level does.
    #[key(numbers(u8, 1, 6))]
    pub levels: Vec<u8>,

    /// The text of a link back to each heading, e.g. `#`. Unset or empty, no link is emitted.
    #[key(custom(
        Text,
        |c: &Self| c.link.clone().into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let text = n.string(t, 0)?;
            c.link = (!text.is_empty()).then_some(text);
            Ok(())
        },
    ))]
    pub link: Option<String>,

    /// Which side of the heading's text that link sits on.
    #[key(choice(Place))]
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
