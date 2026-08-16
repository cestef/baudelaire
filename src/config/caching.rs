//! `caching { }`: the `Cache-Control` the built files are served with.

use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// The `Cache-Control` an uploaded object is served with, enabled by the
/// presence of a `caching { }` block.
#[derive(Debug, Clone, Hash, Default)]
pub struct CacheControl {
    pub enabled: bool,
    /// The value for content-addressed files: everything under the asset
    /// prefix, once `assets { fingerprint }` is on.
    pub immutable: String,
    /// The value for everything else: pages, feeds, and any asset whose name is
    /// not a hash.
    pub default: String,
}

impl CacheControl {
    /// The header value for `key`, or `None` when no policy is configured.
    pub fn header(&self, key: &str, prefix: &str, hashed: bool) -> Option<&str> {
        if !self.enabled {
            return None;
        }
        let immutable = hashed && key.trim_start_matches('/').starts_with(prefix);
        Some(if immutable {
            &self.immutable
        } else {
            &self.default
        })
    }
}

/// The conventional cache policy, filled in by [`Section::SWITCH`] when the
/// `caching { }` block is present and the author named neither value.
impl CacheControl {
    pub(super) const IMMUTABLE: &'static str = "public, max-age=31536000, immutable";
    pub(super) const DEFAULT: &'static str = "public, max-age=0, must-revalidate";
}

/// The top-level `caching { .. }` block: presence turns `Cache-Control` on and
/// fills the defaults, so an untouched or disabled `CacheControl` carries no
/// policy at all and the two states stay distinguishable.
impl Section for CacheControl {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| {
        c.enabled = on;
        if !on {
            return;
        }
        if c.immutable.is_empty() {
            Self::IMMUTABLE.clone_into(&mut c.immutable);
        }
        if c.default.is_empty() {
            Self::DEFAULT.clone_into(&mut c.default);
        }
    });

    const RULES: Block<Self> = Block(&[
        (
            "immutable",
            Text,
            "The `Cache-Control` value for fingerprinted assets, which can be cached forever.",
            |c, n, t| {
                c.immutable = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "default",
            Text,
            "The `Cache-Control` value for everything else.",
            |c, n, t| {
                c.default = n.string(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
