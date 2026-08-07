//! `html { meta { } }`: the head tags a link preview reads.

use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// The `<meta>` description, Open Graph and Twitter tags, and the two facts a
/// build cannot derive for them.
///
/// Everything else in this vocabulary restates something the page already says:
/// its title, its description, its date, its image. These two are the site's own
/// and appear nowhere in a page, so before there was a block for them a site
/// that wanted either had to write the tags by hand in a template, beside the
/// generated ones.
#[derive(Debug, Clone, Hash)]
pub struct MetaConfig {
    /// Emit the tags at all.
    pub enabled: bool,
    /// The site's account, as `twitter:site` wants it: `@handle`.
    ///
    /// A card attributes a link to whoever posted it unless the site says whose
    /// it is, which is the one thing the markup cannot work out from the page.
    pub twitter: Option<String>,
    /// The preview image for a page that names none and gets no generated card.
    ///
    /// Resolved and absolutized like an authored one, so it may be a site-root
    /// path (`/og.png`) or a URL. A page's own `image` always wins, and so does
    /// a card the build drew for it: this is the floor, not an override.
    pub image: Option<String>,
}

impl Default for MetaConfig {
    fn default() -> Self {
        Self {
            // On: the tags restate what the page already says, so a site pays
            // nothing for them and a link to it previews properly.
            enabled: true,
            twitter: None,
            image: None,
        }
    }
}

impl Section for MetaConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "twitter",
            Text,
            "The site's account for `twitter:site`, as `@handle`.",
            |c, n, t| {
                c.twitter = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "image",
            Text,
            "The preview image for a page that names none and gets no generated card.",
            |c, n, t| {
                c.image = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
    ]);
}
