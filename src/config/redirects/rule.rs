//! `redirects { rules { } }`: an old path no page owns, and where it
//! forwards to.

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::dispatch::{Attributed, Attrs};
use crate::config::vocab::attr;
use crate::error::{ConfigError, Result};

/// One declared redirect: where the old path goes, and what the host is told to
/// say about it.
#[derive(Debug, Clone, Hash, Table)]
#[table(
    impl = Attributed,
    const ATTRS: Attrs<Self> = Attrs,
    rule = attr,
    items {
        /// The target, which [`RedirectConfig::item`] reads before the attributes.
        const LEADING: usize = 1;

        fn unkeyed(&self) -> Vec<crate::config::Value> {
            vec![self.target.clone().into()]
        }
    },
)]
pub struct RedirectConfig {
    /// A path on this site, or an absolute URL.
    pub target: String,

    /// The HTTP status the host answers with, `300` to `399`. Defaults to `301`.
    #[key(bounded(u16, 300, 399))]
    pub status: u16,
}

impl RedirectConfig {
    /// The status a redirect takes when it names none, and the only one an HTML
    /// stub can stand in for.
    pub const PERMANENT: u16 = 301;

    /// One `"/old/" "/new/" status=302` line, keyed by its old path.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let old = node.name().value().to_owned();
        if crate::config::Config::traverses(&old) {
            return Err(ConfigError::url_traversal(
                text,
                &old,
                crate::config::node::NodeExt::span(node),
            )
            .into());
        }
        let mut rule = Self {
            target: crate::config::node::NodeExt::string(node, text, 0)?,
            status: Self::PERMANENT,
        };
        rule.read(node, text)?;
        Ok((old, rule))
    }

    /// Whether this redirect says anything a rule file is needed to say: an
    /// HTML stub can only forward a browser, so any non-default status needs
    /// `redirects { file }`.
    pub fn needs_rules(&self) -> bool {
        self.status != Self::PERMANENT
    }
}
