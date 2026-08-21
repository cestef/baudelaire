//! `redirects { }`: the old paths this site still answers for, and how it
//! answers them.

pub mod rule;

use dispatch_derive::Table;

use crate::config::RedirectConfig;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// Where a path that no page owns forwards to, and whether the answer is a rule
/// file or an HTML stub per path.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct RedirectsConfig {
    /// Write a `_redirects` rule file instead of an HTML stub per path. A wildcard or a `status` needs it.
    ///
    /// Both Netlify and Cloudflare Pages serve a static file over a redirect
    /// rule, so a stub would shadow the rule if the two coexisted.
    #[key(flag)]
    pub file: bool,

    /// One line per old path, each naming where its content moved.
    ///
    /// A frontmatter `redirect` covers a page that still exists; these cover a
    /// URL with nothing left to declare it (a deleted page, a generated index).
    #[key(lines(RedirectConfig, "redirect", RedirectConfig::item))]
    pub rules: Vec<(String, RedirectConfig)>,
}
