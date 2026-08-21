//! `html { meta { } }`: the head tags a link preview reads.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// The `<meta>` description, Open Graph and Twitter tags, and the two facts a
/// build cannot derive for them.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct MetaConfig {
    /// Emit the tags at all.
    pub enabled: bool,

    /// The site's account for `twitter:site`, as `@handle`.
    #[key(opt text)]
    pub twitter: Option<String>,

    /// The preview image for a page that names none and gets no generated card.
    ///
    /// A page's own `image` and a generated card both win over it.
    #[key(opt text)]
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
