//! `redirect { }`: an old path no page owns, and where it forwards to.

use kdl::KdlNode;

use crate::config::dispatch::Kind::Number;
use crate::config::dispatch::{Attributed, Attrs};
use crate::config::value::ValueExt;
use crate::error::Result;

/// One declared redirect: where the old path goes, and what the host is told to
/// say about it.
///
/// A line rather than a bare `old -> new` pair because the status is a fact
/// about *this* redirect and not about the site: a permanent move and a
/// temporary diversion are different claims, and a site had no way to make the
/// second one. It is written as an attribute, so the pair a site already wrote
/// keeps its spelling and its meaning.
#[derive(Debug, Clone, Hash)]
pub struct RedirectConfig {
    /// Where the old path forwards to: a path on this site, or an absolute URL.
    pub target: String,
    /// The HTTP status the rule file asks the host to answer with.
    pub status: u16,
}

impl RedirectConfig {
    /// The status a redirect takes when it names none.
    ///
    /// Permanent, because that is what these mostly are: a page moved and the
    /// old URL is not coming back. It is also the only status an HTML stub can
    /// honestly stand in for, since a meta refresh is a client-side round trip
    /// either way and a crawler reads a `301` from the rule file alone.
    pub const PERMANENT: u16 = 301;

    /// One `"/old/" "/new/" status=302` line, keyed by its old path.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let old = node.name().value().to_owned();
        let mut rule = Self {
            // The target is the line's one positional, read here rather than as
            // an attribute: it is what a redirect *is*, and every site already
            // writes it that way.
            target: crate::config::node::NodeExt::string(node, text, 0)?,
            status: Self::PERMANENT,
        };
        rule.read(node, text)?;
        Ok((old, rule))
    }

    /// Whether this redirect says anything a rule file is needed to say.
    ///
    /// An HTML stub forwards a browser and nothing else, so a status other than
    /// the default is a claim only `generate { redirects }` can carry.
    pub fn needs_rules(&self) -> bool {
        self.status != Self::PERMANENT
    }
}

impl Attributed for RedirectConfig {
    /// The target, which [`RedirectConfig::item`] reads before the attributes.
    const LEADING: usize = 1;

    const ATTRS: Attrs<Self> = Attrs(&[(
        "status",
        Number,
        "The HTTP status the host answers with, `300` to `399`. Defaults to `301`.",
        |c, v, t, s| {
            // The redirect class and nothing else: a rule answering `200` or
            // `404` is a different feature under the same file, and one
            // answering `500` is a typo.
            c.status = v.bounded::<u16>(t, s, 300, 399)?;
            Ok(())
        },
    )]);
}
