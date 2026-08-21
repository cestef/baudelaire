//! `artifacts { cards { } }`: generated social cards.

use dispatch_derive::Table;

use crate::config::Basename;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// Generated social cards: the image a link to this site unfurls into, rendered
/// per page from a paged Typst template, where `html.elem` does not exist.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct CardsConfig {
    pub enabled: bool,

    /// The typst template each card is drawn with.
    ///
    /// A file under the templates directory.
    #[key(text)]
    pub template: String,

    /// Card width in pixels.
    ///
    /// The card is one page rendered at one pixel per point, so this is also
    /// the page's width in points.
    #[key(bounded(u32, 1, Self::MAX))]
    pub width: u32,

    /// Card height in pixels.
    #[key(bounded(u32, 1, Self::MAX))]
    pub height: u32,
}

impl CardsConfig {
    /// The directory cards are written to under `dist`, and the leading segment
    /// of every card URL.
    pub const DIR: &'static str = "cards";

    /// The widest and tallest a card may be, so a typo cannot ask for a
    /// gigapixel rasterization.
    pub(crate) const MAX: u32 = 4096;

    /// The served URL of a page's card, whether or not it has been rendered
    /// yet; the meta transform, the renderer and the prune all derive it here.
    pub fn url(&self, permalink: &str) -> String {
        format!("/{}/{}.png", Self::DIR, Basename(permalink))
    }

    /// Whether cards are actually produced: configured *and* compiled in.
    pub fn active(&self) -> bool {
        self.enabled && cfg!(feature = "cards")
    }
}

impl Default for CardsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            template: "card.typ".into(),
            width: 1200,
            height: 630,
        }
    }
}
