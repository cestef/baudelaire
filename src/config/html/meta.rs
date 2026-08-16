//! `html { meta { } }`: the head tags a link preview reads.

use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// The `<meta>` description, Open Graph and Twitter tags, and the two facts a
/// build cannot derive for them.
#[derive(Debug, Clone, Hash)]
pub struct MetaConfig {
    /// Emit the tags at all.
    pub enabled: bool,
    /// The site's account, as `twitter:site` wants it: `@handle`.
    pub twitter: Option<String>,
    /// The preview image for a page that names none and gets no generated card;
    /// a page's own `image` and a generated card both win over it.
    pub image: Option<String>,
}

impl Default for MetaConfig {
    fn default() -> Self {
        Self {
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
