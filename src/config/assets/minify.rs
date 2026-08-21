//! `assets { minify { } }`: which kinds of asset are minified.

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;

/// What the pipeline minifies.
#[derive(Debug, Clone, Copy, Hash, Default, Table)]
#[table(hook(switch = enabled))]
pub struct MinifyConfig {
    /// The answer for a kind the block does not name. A gate beside two
    /// *optional* flags rather than the switch writing them directly, because
    /// [`Section::fill`] runs the switch on every mention: a profile naming one
    /// kind must not re-enable the other.
    enabled: bool,

    /// Minify stylesheets.
    #[key(custom(
        Flag,
        |c: &Self| c.css().into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.css = Some(n.boolean(t, 0)?);
            Ok(())
        },
    ))]
    css: Option<bool>,

    /// Minify JavaScript. Needs `assets { bundle }`, which is what runs it.
    ///
    /// Read by the bundler, so it needs `assets { bundle }` to do anything.
    #[key(custom(
        Flag,
        |c: &Self| c.js().into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.js = Some(n.boolean(t, 0)?);
            Ok(())
        },
    ))]
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
