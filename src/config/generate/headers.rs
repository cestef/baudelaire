//! `generate { headers { } }`: the `_headers` rule file, and the rules an
//! author writes into it themselves.

use kdl::KdlNode;

use crate::config::node::NodeExt;
use crate::error::Result;

/// The `_headers` file Netlify and Cloudflare Pages read from the publish
/// directory: whether it is written, and what the site states in it beyond the
/// policies the build already derives.
///
/// `Cache-Control` comes from `caching { }` and the policy from
/// `security { csp { } }`, because both are answers this build knows and would
/// otherwise be stated twice. Everything else is a claim only the author can
/// make: `X-Robots-Tag` on a staging copy, `X-Frame-Options`, a header some host
/// feature is keyed off. Without these the only way to send one was to write
/// the file by hand after the build, which the sweep then deleted.
#[derive(Debug, Clone, Hash, Default)]
pub struct HeadersConfig {
    /// Whether `_headers` is written at all.
    pub enabled: bool,
    /// A path pattern, and the headers it adds, in the order they were written.
    ///
    /// A `Vec` rather than a map, both levels: the file is read top to bottom,
    /// so the order the author wrote is the order the host applies, and sorting
    /// them would silently reorder a policy.
    pub rules: Vec<(String, Vec<(String, String)>)>,
}

impl HeadersConfig {
    /// Read the flag on the node's own line, then the rules in the block
    /// beneath it.
    ///
    /// Not a [`Section`](crate::config::dispatch::Section), because a section is
    /// a fixed table of keys and neither level here has one: the outer nodes are
    /// path patterns and the inner ones header names, both the author's own
    /// words. It is [`Kind::Tables`](crate::config::dispatch::Kind::Tables) for
    /// exactly that reason.
    ///
    /// A bare `headers` is `#true`, like any flag, so the spelling that only
    /// ever turned the file on still does. A block replaces the rule list
    /// wholesale, as every list in this config does.
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
