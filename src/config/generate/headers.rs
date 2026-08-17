//! `generate { headers { } }`: the `_headers` rule file, and the rules an
//! author writes into it themselves.

use kdl::KdlNode;

use crate::config::node::NodeExt;
use crate::error::Result;

/// The `_headers` file Netlify and Cloudflare Pages read from the publish
/// directory.
///
/// Holds whether it is written and the rules the site states beyond the
/// `Cache-Control` and CSP the build already derives from `caching { }` and
/// `security { csp { } }`.
#[derive(Debug, Clone, Hash, Default)]
pub struct HeadersConfig {
    pub enabled: bool,
    /// A path pattern, and the headers it adds, in the order they were written.
    /// A `Vec` at both levels because the host applies the file top to bottom,
    /// so sorting would silently reorder a policy.
    pub rules: Vec<(String, Vec<(String, String)>)>,
}

impl HeadersConfig {
    /// Read the flag on the node's own line, then the rules in the block
    /// beneath it. Not a [`Section`](crate::config::dispatch::Section), because
    /// neither level has a fixed key table: the outer nodes are path patterns
    /// and the inner ones header names, both the author's own words.
    pub(super) fn fill(&mut self, node: &KdlNode, text: &str) -> Result<()> {
        self.enabled = node.boolean(text, 0)?;
        let Some(block) = node.children() else {
            return Ok(());
        };
        self.rules = block
            .nodes()
            .iter()
            .map(|rule| Ok((rule.name().value().to_owned(), rule.pairs(text)?)))
            .collect::<Result<_>>()?;
        Ok(())
    }
}
