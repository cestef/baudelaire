//! `typst { }`: engine knobs, `sys.inputs`, fonts, and the package registry.

pub mod fonts;

use crate::config::FontConfig;
use crate::config::dispatch::Kind::{Block as Nested, Table, Toggles, Url};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// Typst engine knobs.
#[derive(Debug, Clone, Hash, Default)]
pub struct TypstConfig {
    /// Extra experimental Typst features to enable (e.g. `a11y-extras`). `html`
    /// is always forced on in `world.rs`, so this list is purely additive and
    /// never needs to include it.
    pub features: Vec<String>,
    /// Typst `sys.inputs` entries.
    pub inputs: Vec<(String, String)>,
    /// A mirror of Typst Universe to download the `preview` namespace from,
    /// without the trailing slash. `None` is the official registry.
    ///
    /// It covers a page's own `#import` and the site's theme alike, since both
    /// resolve through the same store. Only `preview` is affected: every other
    /// namespace is served from the local package directories and never fetched.
    pub registry: Option<String>,
    /// Where a compile looks for glyphs.
    pub fonts: FontConfig,
}

/// The `typst { .. }` section: typst engine knobs.
impl Section for TypstConfig {
    const RULES: Block<Self> = Block(&[
        (
            "features",
            Toggles,
            "Typst language features to enable, or `-name` to disable one. `html` cannot be removed.",
            |c, n, t| {
                c.features = n.features(t)?;
                Ok(())
            },
        ),
        (
            "inputs",
            Table,
            "Values passed to every compile as `sys.inputs`, one `key value` line per entry.",
            |c, n, t| {
                c.inputs = n.pairs(t)?;
                Ok(())
            },
        ),
        // Stored without its trailing slash: the store joins `/preview/..` onto
        // it, and a doubled slash is a 404 from some hosts and a redirect from
        // others.
        (
            "fonts",
            Nested(FontConfig::rows),
            "Where a compile looks for glyphs.",
            |c, n, t| c.fonts.fill(n, t),
        ),
        (
            "registry",
            Url,
            "Where typst packages are fetched from.",
            |c, n, t| {
                c.registry = Some(n.url(t, 0)?.trim_end_matches('/').to_owned());
                Ok(())
            },
        ),
    ]);
}
