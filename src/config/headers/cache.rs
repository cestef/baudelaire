//! `headers { cache { } }`: the `Cache-Control` the built files are served
//! with.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section, Switch};
use crate::config::vocab::rule;

/// The `cache { .. }` block: presence turns `Cache-Control` on and
/// fills the defaults, so an untouched or disabled `CacheControl` carries no
/// policy at all and the two states stay distinguishable.
#[derive(Debug, Clone, Hash, Default, Table)]
#[table(items {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| {
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
        },
        on: |c| c.enabled,
    });
})]
pub struct CacheControl {
    pub enabled: bool,

    /// The `Cache-Control` value for fingerprinted assets, which can be cached forever.
    ///
    /// Everything under the asset prefix, once `assets { fingerprint }` is on.
    #[key(text)]
    pub immutable: String,

    /// The `Cache-Control` value for everything else.
    ///
    /// Pages, feeds, and any asset whose name is not a hash.
    #[key(text)]
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
/// `cache { }` block is present and the author named neither value.
impl CacheControl {
    pub(super) const IMMUTABLE: &'static str = "public, max-age=31536000, immutable";
    pub(super) const DEFAULT: &'static str = "public, max-age=0, must-revalidate";
}
