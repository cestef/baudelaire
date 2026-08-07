//! `html { region { } }`: which part of a rendered page is its prose.

use crate::config::dispatch::Kind::{Text, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// The element a page's own prose lives in, and the chrome inside it that is
/// not prose.
///
/// A fact about the markup a site emits, which is why it sits here rather than
/// under the one processor that first needed it. It used to be
/// `generate { search { region } }`, and a feed carrying a page's body had to
/// either read the search block (which is not what it is) or ask the same
/// question a second time under a second name.
///
/// Tag names rather than selectors. A selector engine would be a second HTML
/// parser, and the question is "which landmark", which a name answers: `main`
/// for the default layout, `article` for one that binds its prose to that,
/// `body` for a page that is nothing but prose.
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
    ///
    /// Named here because this is the type that owns the setting: the extractor
    /// spelled the same word again for its own default, which is two statements
    /// of one fact and exactly what moving `region` under `html` was meant to
    /// end.
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
