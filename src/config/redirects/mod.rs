//! `redirects { }`: the old paths this site still answers for, and how it
//! answers them.

pub mod rule;

use crate::config::dispatch::Kind::{Flag, Lines};
use crate::config::dispatch::{Attributed, Block, Section};
use crate::config::node::NodeExt;
use crate::config::{RedirectConfig, Value};

/// Where a path that no page owns forwards to, and whether the answer is a rule
/// file or an HTML stub per path.
#[derive(Debug, Clone, Hash, Default)]
pub struct RedirectsConfig {
    /// Write a `_redirects` file in place of the per-path HTML stubs; both
    /// Netlify and Cloudflare Pages serve a static file over a redirect rule,
    /// so a stub would shadow the rule if the two coexisted.
    pub file: bool,
    /// Old paths with no page behind them, each paired with where it moved.
    ///
    /// A frontmatter `redirect` covers a page that still exists; these cover a
    /// URL with nothing left to declare it (a deleted page, a generated index).
    pub rules: Vec<(String, RedirectConfig)>,
}

impl Section for RedirectsConfig {
    const RULES: Block<Self> = Block(&[
        (
            "file",
            Flag,
            "Write a `_redirects` rule file instead of an HTML stub per path. A wildcard or a `status` needs it.",
            |c| c.file.into(),
            |c, n, t| {
                c.file = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "rules",
            Lines(RedirectConfig::rows),
            "One line per old path, each naming where its content moved.",
            |c| Value::each(&c.rules, Attributed::values),
            |c, n, t| {
                c.rules = n.unique(t, "redirect", RedirectConfig::item)?;
                Ok(())
            },
        ),
    ]);
}
