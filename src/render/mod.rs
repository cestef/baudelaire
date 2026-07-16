//! Render layer: post-processes compiled documents before serialization.
//!
//! Post-processing operates on typst-html's own typed DOM
//! ([`typst_html::HtmlDocument`]), never on the serialized string, honoring the
//! project rule that HTML is never manipulated as text.

mod anchors;
mod asset;
mod base;
mod embed;
mod fingerprint;
mod image;
mod links;
mod meta;
mod rewrite;
mod standard;
mod transform;

pub use asset::AssetMap;
pub use links::LinkMap;

use crate::graph::Fingerprint;

/// A raw `href`/`src` split at its `#fragment` / `?query` boundary: the one
/// parsing rule for URL tails, shared by link and asset resolution.
pub(crate) struct Tail<'a> {
    /// The path portion, up to the first `#` or `?`.
    pub path: &'a str,
    /// The trailing `#fragment` / `?query`, empty when absent.
    pub tail: &'a str,
}

impl<'a> Tail<'a> {
    pub fn of(raw: &'a str) -> Self {
        let (path, tail) = match raw.find(['#', '?']) {
            Some(i) => raw.split_at(i),
            None => (raw, ""),
        };
        Self { path, tail }
    }
}

use typst_html::HtmlDocument;

use crate::config::Config;
use crate::content::Page;
use crate::render::transform::{Cx, Transforms};

/// The site-wide render context. Built once per build from the full page set,
/// then shared read-only across the parallel compile pool.
pub struct Renderer {
    links: LinkMap,
    assets: AssetMap,
    transforms: Transforms,
}

impl Renderer {
    /// Build a renderer that resolves links across `pages` and rewrites asset
    /// references through `assets` (the processed-asset URL map). `root` is the
    /// typst project root absolute link paths resolve against.
    pub fn new(pages: &[Page], assets: AssetMap, root: &std::path::Path) -> Self {
        Self {
            links: LinkMap::new(pages, root),
            assets,
            transforms: Transforms::builtin(),
        }
    }

    /// Fingerprint of the page-to-permalink map, for the build cache.
    pub fn links(&self) -> crate::graph::Hash {
        self.links.fingerprint()
    }

    /// The processed-asset URL map (for the build-cache fingerprint).
    pub fn assets(&self) -> &AssetMap {
        &self.assets
    }

    /// Run the DOM transform pipeline over a page's document in place: link
    /// resolution (source-path `.typ` links to permalinks) first, then the
    /// configured transforms. Returns the raw targets of any internal `.typ`
    /// links that point at a non-existent page.
    pub fn rewrite(&self, doc: &mut HtmlDocument, page: &Page, config: &Config) -> Vec<String> {
        let mut cx = Cx {
            config,
            page,
            links: &self.links,
            assets: &self.assets,
            broken: Vec::new(),
        };
        self.transforms.apply(doc, &mut cx);
        cx.broken
    }
}
