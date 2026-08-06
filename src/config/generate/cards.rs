//! `generate { cards { } }`: generated social cards.

use crate::config::Basename;
use crate::config::dispatch::Kind::{Number, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Generated social cards: the image a link to this site unfurls into, rendered
/// per page from a Typst template. Enabled by the presence of a
/// `generate { cards { .. } }` block.
///
/// The template is compiled to a *paged* document, not an HTML one, so it is
/// ordinary Typst: `html.elem` does not exist there, and page layout does.
#[derive(Debug, Clone, Hash)]
pub struct CardsConfig {
    /// Whether to render cards.
    pub enabled: bool,
    /// The template file under the templates directory.
    pub template: String,
    /// Card size in pixels. The card is one page rendered at one pixel per
    /// point, so these are also the page's dimensions in points.
    pub width: u32,
    pub height: u32,
}

impl CardsConfig {
    /// The directory cards are written to under `dist`, and the leading segment
    /// of every card URL.
    pub const DIR: &'static str = "cards";

    /// The widest and tallest a card may be. Unfurlers cap well below this; the
    /// limit exists so a typo cannot ask for a gigapixel rasterization.
    pub(crate) const MAX: u32 = 4096;

    /// The served URL of a page's card, whether or not it has been rendered
    /// yet: the meta transform names it while the file is still being made, the
    /// renderer writes it, and the prune keeps it, so all three have to derive
    /// it the same way.
    pub fn url(&self, permalink: &str) -> String {
        format!("/{}/{}.png", Self::DIR, Basename(permalink))
    }

    /// Whether cards are actually produced: configured *and* compiled in. A
    /// build without the `cards` feature has no rasterizer, so pointing pages at
    /// images it cannot make would be worse than making none.
    pub fn active(&self) -> bool {
        self.enabled && cfg!(feature = "cards")
    }
}

impl Default for CardsConfig {
    fn default() -> Self {
        // opt-in: rendering a page per card is the most expensive thing a build
        // can do per page. 1200x630 is the size every unfurler crops to.
        Self {
            enabled: false,
            template: "card.typ".into(),
            width: 1200,
            height: 630,
        }
    }
}

/// The `cards { template ..; width ..; height .. }` block. Its presence enables
/// social card rendering.
impl Section for CardsConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "template",
            Text,
            "The typst template each card is drawn with.",
            |c, n, t| {
                c.template = n.string(t, 0)?;
                Ok(())
            },
        ),
        ("width", Number, "Card width in pixels.", |c, n, t| {
            c.width = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, Self::MAX)?;
            Ok(())
        }),
        ("height", Number, "Card height in pixels.", |c, n, t| {
            c.height = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, Self::MAX)?;
            Ok(())
        }),
    ]);
}
