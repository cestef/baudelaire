//! `assets { minify { } }`: which kinds of asset are minified.

use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// What the pipeline minifies.
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct MinifyConfig {
    /// The answer for a kind the block does not name. A gate beside two
    /// *optional* flags rather than the switch writing them directly, because
    /// [`Section::fill`] runs the switch on every mention: a profile naming one
    /// kind must not re-enable the other.
    enabled: bool,
    /// Minify stylesheets, or `None` to follow the switch.
    css: Option<bool>,
    /// Minify JavaScript, or `None` to follow the switch. Read by the bundler,
    /// so it needs `assets { bundle }` to do anything.
    js: Option<bool>,
}

impl MinifyConfig {
    pub fn css(self) -> bool {
        self.css.unwrap_or(self.enabled)
    }

    /// Whether JavaScript is minified; still needs a bundler to do it, see
    /// [`AssetConfig::bundling`](crate::config::AssetConfig::bundling).
    pub fn js(self) -> bool {
        self.js.unwrap_or(self.enabled)
    }

    pub fn any(self) -> bool {
        self.css() || self.js()
    }
}

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
