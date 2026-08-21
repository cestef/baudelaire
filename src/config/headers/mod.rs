//! `headers { }`: what a host is told about the built files, and the `_headers`
//! rule file that states it.

pub mod cache;

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Tables;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;
use crate::config::{CacheControl, Value};
use crate::error::Result;

/// The headers the built files are served with: the `Cache-Control` policy
/// every destination applies, and the rules the site states beyond it.
#[derive(Debug, Clone, Default, Table)]
#[table(hook(switch = file))]
pub struct HeadersConfig {
    /// Write the `_headers` file Netlify and Cloudflare Pages read from the
    /// publish directory. On with the block's presence, so a site that only
    /// wants the policy writes `headers #false { cache { } }`.
    pub file: bool,

    /// The `Cache-Control` the built files are served with, by this file and by every destination that can say so. Its presence turns it on; `#false` turns it off again.
    #[key(nested(CacheControl))]
    pub cache: CacheControl,

    /// Headers of the site's own, one block per path pattern, applied before the derived ones.
    ///
    /// A `Vec` at both levels because the host applies the file top to bottom,
    /// so sorting would silently reorder a policy.
    #[key(custom(Tables, Self::written, Self::read))]
    pub rules: Vec<(String, Vec<(String, String)>)>,
}

impl HeadersConfig {
    /// The rules read back: one entry per path pattern, each holding the
    /// headers written under it.
    fn written(&self) -> Value {
        Value::each(&self.rules, |headers| {
            Value::each(headers, |value| value.clone().into())
        })
    }

    /// Read a `rules { }` block: the outer nodes are path patterns and the
    /// inner ones header names, both the author's own words, so neither level
    /// has a key table to dispatch against.
    fn read(&mut self, node: &KdlNode, text: &str) -> Result<()> {
        self.rules = node
            .block(text)?
            .nodes()
            .iter()
            .map(|rule| Ok((rule.name().value().to_owned(), rule.pairs(text)?)))
            .collect::<Result<_>>()?;
        Ok(())
    }
}

/// `cache` is left out: the policy names no page, and the file carrying it is
/// written by a processor that runs whatever the page cache says.
impl std::hash::Hash for HeadersConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            file,
            cache: _,
            rules,
        } = self;
        (file, rules).hash(state);
    }
}
