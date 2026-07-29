//! The mapping from asset request paths to their processed output URLs.
//!
//! Built by the engine's asset pipeline (minify -> bundle -> fingerprint) and
//! shared read-only into the render layer, where the [`super::fingerprint`]
//! transform rewrites `href`/`src` references to point at the processed files.

use std::collections::BTreeMap;

use crate::render::Tail;

/// Maps an asset's authored request path (`/assets/style.css`) to the URL it is
/// actually served at (`/assets/style.<hash>.css`). Identity for assets whose
/// name is unchanged (minify-only, no fingerprint): those need no rewrite and
/// are simply absent from the map.
///
/// Ordered so a page's recorded probes and the current map compare the same way
/// every build.
#[derive(Debug, Default, Clone, Hash)]
pub struct AssetMap {
    map: BTreeMap<String, String>,
    /// The URL prefix every key of `map` starts with, so a reference that could
    /// never name an asset is not recorded as depending on one.
    ///
    /// Taken from the pipeline that builds the keys rather than re-derived, so
    /// the two cannot drift: a site that renames its asset directory or sets a
    /// base path moves both at once.
    prefix: String,
}

/// The asset-map entries a page's references consulted: for each request path
/// probed, the URL it was served at, or `None` when nothing was mapped there.
///
/// A page's dependency on the processed-asset tree, which the per-page tracker
/// cannot see: renames happen in the render pass, and typst never reads the
/// processed files. Recorded per page and revalidated on a cache hit, so
/// re-fingerprinting one asset rebuilds the pages that reference it instead of
/// the whole site.
///
/// The `None` entries are load-bearing: a reference to an asset that does not
/// exist yet must invalidate the page once the asset appears, or the page keeps
/// serving the unrewritten URL.
pub type AssetDeps = BTreeMap<String, Option<String>>;

/// Where a reference is served from, and the entry that decided it.
///
/// Returned together so a caller cannot read the URL without also being handed
/// the dependency the lookup just created.
pub struct Served {
    /// The processed URL with any `#fragment`/`?query` restored, or `None` when
    /// the reference names no mapped asset.
    pub url: Option<String>,
    /// The entry consulted, empty for a reference that could never be an asset.
    pub probed: AssetDeps,
}

impl AssetMap {
    /// An empty map whose keys will all start with `prefix`, the same string the
    /// asset pipeline builds its URLs from.
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

    /// The recorded `request -> served` pairs, for exposing the map to client JS
    /// (the `baudelaire:assets` virtual module).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Resolve a raw `href`/`src` to its processed URL, preserving any trailing
    /// `#fragment` / `?query`, together with the dependency the lookup creates.
    ///
    /// A reference outside the asset prefix (an external URL, a page link, a
    /// bare fragment) records nothing: no such path can ever become a key, so
    /// depending on its absence would only bloat every page's entry.
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

    /// Every recorded pair, for the build cache to revalidate a page's recorded
    /// [`AssetDeps`] against.
    pub fn served(&self) -> &BTreeMap<String, String> {
        &self.map
    }
}
