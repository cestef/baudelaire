//! `assets { minify { } }`: which kinds of asset are minified.

use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// What the pipeline minifies.
///
/// One flag covered both for a long time, and the two are not one decision: CSS
/// is minified by lightningcss on its own, while JavaScript is minified by the
/// bundler and so only when `assets { bundle }` is on. A site that wanted small
/// stylesheets and readable, un-mangled scripts had to choose.
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct MinifyConfig {
    /// Minify stylesheets (lightningcss).
    pub css: bool,
    /// Minify JavaScript. Read by the bundler, so it needs `assets { bundle }`:
    /// without it scripts are copied verbatim and nothing minifies them.
    pub js: bool,
}

impl MinifyConfig {
    /// Whether anything is minified at all: what a gate asks before reporting a
    /// capability this binary lacks.
    pub fn any(self) -> bool {
        self.css || self.js
    }
}

/// The block's presence turns every kind on, and `minify #false` takes them all
/// back off: the same switch every section has, so the one-flag spelling a site
/// already wrote keeps meaning what it did.
impl Section for MinifyConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| {
        c.css = on;
        c.js = on;
    });

    const RULES: Block<Self> = Block(&[
        ("css", Flag, "Minify stylesheets.", |c, n, t| {
            c.css = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "js",
            Flag,
            "Minify JavaScript. Needs `assets { bundle }`, which is what runs it.",
            |c, n, t| {
                c.js = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
