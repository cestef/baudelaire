//! `caching { }`: the `Cache-Control` the built files are served with.

use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// The `Cache-Control` an uploaded object is served with, and the reason
/// `assets { fingerprint }` is worth turning on.
///
/// Fingerprinting renames a file after its own content, which makes it safe to
/// cache forever: a change produces a different name, so a stale copy is never
/// the one asked for. A raw bucket sets no `Cache-Control` at all, though, and
/// Netlify/Vercel/Cloudflare Pages only guess. Without this the whole point of
/// hashing a filename is thrown away at the last step.
///
/// Enabled by the presence of a `caching { }` block; both values have defaults, so
/// a bare `cache` is the sensible policy.
#[derive(Debug, Clone, Hash, Default)]
pub struct CacheControl {
    /// Whether to send `Cache-Control` at all.
    pub enabled: bool,
    /// For content-addressed files: everything under the asset prefix, once
    /// `assets { fingerprint }` is on. Cached indefinitely.
    pub immutable: String,
    /// For everything else: pages, feeds, `robots.txt`, and any asset whose name
    /// is not a hash. Revalidated, because these keep their names across builds.
    pub default: String,
}

impl CacheControl {
    /// The header value for `key`, or `None` when no policy is configured.
    ///
    /// `hashed` says whether this build content-addresses its assets; without
    /// it, a file under the asset prefix keeps its authored name across builds
    /// and is exactly as mutable as a page.
    pub fn header(&self, key: &str, prefix: &str, hashed: bool) -> Option<&str> {
        if !self.enabled {
            return None;
        }
        let immutable = hashed && key.trim_start_matches('/').starts_with(prefix);
        Some(match immutable {
            true => &self.immutable,
            false => &self.default,
        })
    }
}

/// The conventional cache policy, applied when `deploy { s3 { cache } }` is
/// present. A year is the maximum `max-age` any cache honours, and `immutable`
/// stops a reload from revalidating a file that cannot have changed.
impl CacheControl {
    pub(super) const IMMUTABLE: &'static str = "public, max-age=31536000, immutable";
    pub(super) const DEFAULT: &'static str = "public, max-age=0, must-revalidate";
}

/// The top-level `caching { .. }` block: presence turns `Cache-Control` on, and
/// both values default to the conventional policy.
impl Section for CacheControl {
    fn enable(&mut self) -> bool {
        self.enabled = true;
        // Filled here rather than in `Default`, so an untouched `S3Config`
        // carries no policy at all and the two states stay distinguishable.
        if self.immutable.is_empty() {
            Self::IMMUTABLE.clone_into(&mut self.immutable);
        }
        if self.default.is_empty() {
            Self::DEFAULT.clone_into(&mut self.default);
        }
        true
    }

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
