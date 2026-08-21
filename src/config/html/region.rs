//! `html { region { } }`: which part of a rendered page is its prose.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// The element a page's own prose lives in, and the chrome inside it that is
/// not prose.
#[derive(Debug, Clone, Hash, Table)]
pub struct RegionConfig {
    /// The element whose contents are the page's prose, by tag name. A page without one counts whole.
    #[key(text)]
    pub element: String,

    /// Elements to leave out of it, by tag name, one word each.
    ///
    /// The chrome a layout puts *inside* its content region.
    #[key(texts)]
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
