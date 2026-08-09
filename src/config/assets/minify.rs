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
    /// What the block's presence turns on, and `minify #false` takes back off:
    /// the answer for a kind the block does not name.
    ///
    /// A gate beside two *optional* flags, rather than the switch writing the
    /// flags directly, which was this section's alone among the switchable
    /// ones. That is exactly what broke: [`Section::fill`] runs the switch on
    /// *every* mention, so a profile naming one half re-enabled the other.
    /// `assets { minify { js #false } }` with a profile adding
    /// `minify { css #true }` minified JavaScript after all, which is a
    /// fill-in-place violation and the one thing a profile overlay must never
    /// do. A kind the author has named keeps its answer, so re-running the
    /// switch says nothing about it.
    enabled: bool,
    /// Minify stylesheets (lightningcss), or `None` to follow the switch.
    css: Option<bool>,
    /// Minify JavaScript, or `None` to follow the switch. Read by the bundler,
    /// so it needs `assets { bundle }`: without it scripts are copied verbatim
    /// and nothing minifies them.
    js: Option<bool>,
}

impl MinifyConfig {
    /// Whether stylesheets are minified.
    pub fn css(self) -> bool {
        self.css.unwrap_or(self.enabled)
    }

    /// Whether JavaScript is minified. Still needs a bundler to do it; see
    /// [`AssetConfig::bundling`](crate::config::AssetConfig::bundling).
    pub fn js(self) -> bool {
        self.js.unwrap_or(self.enabled)
    }

    /// Whether anything is minified at all: what a gate asks before reporting a
    /// capability this binary lacks.
    pub fn any(self) -> bool {
        self.css() || self.js()
    }
}

/// The block's presence turns minification on, and `minify #false` takes it back
/// off: the same switch every section has, so the one-flag spelling a site
/// already wrote keeps meaning what it did.
impl Section for MinifyConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        ("css", Flag, "Minify stylesheets.", |c, n, t| {
            c.css = Some(n.boolean(t, 0)?);
            Ok(())
        }),
        (
            "js",
            Flag,
            "Minify JavaScript. Needs `assets { bundle }`, which is what runs it.",
            |c, n, t| {
                c.js = Some(n.boolean(t, 0)?);
                Ok(())
            },
        ),
    ]);
}
