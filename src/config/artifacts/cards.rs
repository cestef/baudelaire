//! `artifacts { cards { } }`: generated social cards.

use crate::config::Basename;
use crate::config::dispatch::Kind::{Number, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Generated social cards: the image a link to this site unfurls into, rendered
/// per page from a paged Typst template, where `html.elem` does not exist.
#[derive(Debug, Clone, Hash)]
pub struct CardsConfig {
    pub enabled: bool,
    /// The template file under the templates directory.
    pub template: String,
    /// Card size in pixels; the card is one page rendered at one pixel per
    /// point, so these are also the page's dimensions in points.
    pub width: u32,
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

/// The `cards { }` block, whose presence enables social card rendering.
impl Section for CardsConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "template",
            Text,
            "The typst template each card is drawn with.",
            |c| c.template.clone().into(),
            |c, n, t| {
                c.template = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "width",
            Number,
            "Card width in pixels.",
            |c| c.width.into(),
            |c, n, t| {
                c.width = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, Self::MAX)?;
                Ok(())
            },
        ),
        (
            "height",
            Number,
            "Card height in pixels.",
            |c| c.height.into(),
            |c, n, t| {
                c.height = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, Self::MAX)?;
                Ok(())
            },
        ),
    ]);
}
