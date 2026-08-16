//! The mapping from asset request paths to their processed output URLs.
//!
//! Built by the engine's asset pipeline and shared read-only into the render
//! layer, where references are rewritten to point at the processed files.

use std::collections::BTreeMap;

use crate::render::Tail;

/// Maps an asset's authored request path (`/assets/style.css`) to the URL it is
/// actually served at (`/assets/style.<hash>.css`). An asset whose name is
/// unchanged needs no rewrite and is simply absent from the map.
#[derive(Debug, Default, Clone, Hash)]
pub struct AssetMap {
    map: BTreeMap<String, String>,
    /// The URL prefix every key of `map` starts with, so a reference that could
    /// never name an asset is not recorded as depending on one.
    prefix: String,
}

/// The asset-map entries a page's references consulted: for each request path
/// probed, the URL it was served at, or `None` when nothing was mapped there.
///
/// The `None` entries are load-bearing: a reference to an asset that does not
/// exist yet must invalidate the page once the asset appears.
pub type AssetDeps = BTreeMap<String, Option<String>>;

/// Where a reference is served from, and the entry that decided it.
pub struct Served {
    /// The processed URL with any `#fragment`/`?query` restored, or `None` when
    /// the reference names no mapped asset.
    pub url: Option<String>,
    /// The entry consulted, empty for a reference that could never be an asset.
    pub probed: AssetDeps,
}

impl AssetMap {
    /// An empty map whose keys will all start with `prefix`.
    pub fn new(prefix: String) -> Self {
        Self {
            map: BTreeMap::new(),
            prefix,
        }
    }

    /// Record that `from` (a request path) is served as `to`.
    pub fn insert(&mut self, from: String, to: String) {
        self.map.insert(from, to);
    }

    /// The recorded `request -> served` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Resolve a raw `href`/`src` to its processed URL, preserving any trailing
    /// `#fragment` / `?query`, together with the dependency the lookup creates.
    ///
    /// A reference outside the asset prefix records nothing: no such path can
    /// ever become a key, so depending on its absence would only bloat every
    /// page's entry.
    pub fn resolve(&self, raw: &str) -> Served {
        let split = Tail::of(raw);
        let served = self.map.get(split.path);
        let url = served.map(|url| format!("{url}{}", split.tail));
        let probed = if self.owns(split.path) {
            AssetDeps::from([(split.path.to_owned(), served.cloned())])
        } else {
            AssetDeps::new()
        };
        Served { url, probed }
    }

    /// Whether `path` is spelled such that it could name an asset at all.
    fn owns(&self, path: &str) -> bool {
        !self.prefix.is_empty() && path.starts_with(&self.prefix)
    }

    /// Every recorded pair, for revalidating a page's [`AssetDeps`] against.
    pub fn served(&self) -> &BTreeMap<String, String> {
        &self.map
    }
}
