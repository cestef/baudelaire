//! `html { anchors { } }`: heading ids, and the link back to them.

use crate::config::Named;
use crate::config::dispatch::Kind::{Choice, Numbers, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Where a heading's self link sits relative to the heading's own text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Place {
    /// After the text, which is where a reader looks for one and where the `#`
    /// convention puts it.
    #[default]
    After,
    /// Before the text, in the margin a stylesheet can pull it out into.
    Before,
}

impl Named for Place {
    const NAMES: &'static [(&'static str, Self)] =
        &[("after", Self::After), ("before", Self::Before)];
}

/// Deep-linkable headings: which get an `id`, and whether a reader is given
/// something to click to copy it.
///
/// The id half has always been here. The link half had not been built, and the
/// key documented itself as doing both, so every site that turned anchors on got
/// half of what it read about.
#[derive(Debug, Clone, Hash)]
pub struct AnchorConfig {
    /// Derive an `id` for a heading that has none. An authored `id` is always
    /// left alone.
    pub enabled: bool,
    /// The heading levels that get one, as `1`..`6`. Empty means every level.
    ///
    /// A page title is often an `<h1>` a layout emits, and a table of contents
    /// rarely wants every `<h6>`: naming the levels is how a site says which
    /// headings are section headings.
    pub levels: Vec<u8>,
    /// The text of the link back to a heading, e.g. `#` or `¶`. `None` emits no
    /// link, which is what every site got before this existed.
    pub link: Option<String>,
    /// Which side of the heading's text that link sits on.
    pub place: Place,
}

impl AnchorConfig {
    /// The class the self link carries, so a stylesheet has one name to reach it
    /// by. Fixed rather than configured: it is markup this crate emits, like the
    /// `sx-` highlight classes, and a name a site can change is a name a theme
    /// cannot rely on.
    pub const CLASS: &'static str = "anchor";

    /// Whether a heading at `level` gets an id. An empty `levels` is every level,
    /// not none: the list narrows, and a site that never wrote one asked for no
    /// narrowing.
    pub fn covers(&self, level: u8) -> bool {
        self.levels.is_empty() || self.levels.contains(&level)
    }
}

impl Default for AnchorConfig {
    fn default() -> Self {
        Self {
            // On: a heading nothing can link to is a heading a reader cannot
            // share, and deriving one costs nothing.
            enabled: true,
            levels: Vec::new(),
            // Off: markup inside a heading is markup the site did not write, and
            // a theme with no styling for it gets a stray `#` in its titles.
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
                // An empty text is no link, not an empty one: an `<a>` with
                // nothing in it is a target no reader can click, on every
                // heading of every page. It is also the only way to say *off*
                // once a theme's own config has said `link "#"`, since `fill`
                // fills in place and this key takes a string.
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
