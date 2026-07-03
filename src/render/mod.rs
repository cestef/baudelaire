//! Render layer: post-processes compiled documents before serialization.
//!
//! Post-processing operates on typst-html's own typed DOM
//! ([`typst_html::HtmlDocument`]), never on the serialized string, honoring the
//! project rule that HTML is never manipulated as text.

mod asset;
mod embed;
mod fingerprint;
mod links;
mod rewrite;
mod transform;

pub use asset::AssetMap;
pub use links::LinkMap;

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
    /// references through `assets` (the processed-asset URL map).
    pub fn new(pages: &[Page], assets: AssetMap) -> Self {
        Self {
            links: LinkMap::new(pages),
            assets,
            transforms: Transforms::builtin(),
        }
    }

    /// Run the DOM transform pipeline over a page's document in place: link
    /// resolution (source-path `.typ` links → permalinks) first, then the
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
