//! The mapping from asset request paths to their processed output URLs.
//!
//! Built by the engine's asset pipeline (minify → bundle → fingerprint) and
//! shared read-only into the render layer, where the [`super::fingerprint`]
//! transform rewrites `href`/`src` references to point at the processed files.

use std::collections::BTreeMap;

use crate::graph::{Fingerprint, Hash};

/// Maps an asset's authored request path (`/assets/style.css`) to the URL it is
/// actually served at (`/assets/style.<hash>.css`). Identity for assets whose
/// name is unchanged (minify-only, no fingerprint) — those need no rewrite and
/// are simply absent from the map.
///
/// Ordered so its hash is deterministic: the map folds into the build cache
/// fingerprint, invalidating pages when an asset is re-fingerprinted.
#[derive(Debug, Default, Clone, Hash)]
pub struct AssetMap {
    map: BTreeMap<String, String>,
}

impl Fingerprint for AssetMap {
    /// The map's content digest, folded into the build cache so pages rebuild
    /// when an asset is re-fingerprinted. Deterministic via the ordered map.
    fn fingerprint(&self) -> Hash {
        Hash::of(self)
    }
}

impl AssetMap {
    /// Record that `from` (a request path) is served as `to`.
    pub fn insert(&mut self, from: String, to: String) {
        self.map.insert(from, to);
    }

    /// The recorded `request → served` pairs, for exposing the map to client JS
    /// (the `baudelaire:assets` virtual module).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Resolve a raw `href`/`src` to its processed URL, preserving any trailing
    /// `#fragment` / `?query`. `None` if the reference is not a mapped asset.
    pub fn resolve(&self, raw: &str) -> Option<String> {
        let split = super::Tail::of(raw);
        self.map
            .get(split.path)
            .map(|url| format!("{url}{}", split.tail))
    }
}
