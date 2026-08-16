//! `html { region { } }`: which part of a rendered page is its prose.

use crate::config::dispatch::Kind::{Text, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// The element a page's own prose lives in, and the chrome inside it that is
/// not prose.
#[derive(Debug, Clone, Hash)]
pub struct RegionConfig {
    /// The element whose contents are the page's prose, by tag name. Empty
    /// means the whole document.
    pub element: String,
    /// Elements dropped wherever they occur inside it, by tag name: the chrome
    /// a layout puts *inside* its content region.
    pub ignore: Vec<String>,
}

impl RegionConfig {
    /// The landmark a page's prose lives in under any conventional layout, and
    /// the one every consumer falls back to.
    pub const MAIN: &'static str = "main";
}

impl Default for RegionConfig {
    fn default() -> Self {
        Self {
            element: Self::MAIN.into(),
            ignore: Vec::new(),
        }
    }
}

impl Section for RegionConfig {
    const RULES: Block<Self> = Block(&[
        (
            "element",
            Text,
            "The element whose contents are the page's prose, by tag name. A page without one counts whole.",
            |c, n, t| {
                c.element = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "ignore",
            Texts,
            "Elements to leave out of it, by tag name, one word each.",
            |c, n, t| {
                c.ignore = n.words(t)?;
                Ok(())
            },
        ),
    ]);
}
