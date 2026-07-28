//! Render layer: post-processes compiled documents before serialization.
//!
//! Post-processing operates on typst-html's own typed DOM
//! ([`typst_html::HtmlDocument`]), never on the serialized string, honoring the
//! project rule that HTML is never manipulated as text.

mod anchors;
mod asset;
mod base;
mod embed;
mod externalize;
mod fingerprint;
mod fragment;
mod image;
mod lang;
mod links;
mod meta;
mod rewrite;
mod sources;
mod srcset;
mod standard;
mod transform;

pub use asset::AssetMap;
pub use externalize::ImageRef;
pub use fragment::Fragments;
pub use links::LinkMap;
pub use srcset::SrcSets;

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
    srcsets: SrcSets,
    /// Project root, so the externalize transform resolves an image marker's
    /// project-relative path to the source file on disk.
    root: std::path::PathBuf,
    transforms: Transforms,
}

/// The findings of running the transform pipeline over one page: internal links
/// with no target, and images externalized out of the DOM into files.
pub struct Rewrite {
    /// Raw targets of internal `.typ` links that point at a non-existent page.
    pub broken: Vec<String>,
    /// Images lifted out of the DOM, for the engine to copy into `dist`.
    pub images: Vec<ImageRef>,
}

impl Renderer {
    /// Build a renderer that resolves links across `pages` and rewrites asset
    /// references through `assets` (the processed-asset URL map), adding a
    /// `srcset` to each image with variants recorded in `srcsets`. `root` is the
    /// typst project root absolute link paths resolve against.
    pub fn new(pages: &[Page], assets: AssetMap, srcsets: SrcSets, root: &std::path::Path) -> Self {
        Self {
            links: LinkMap::new(pages, root),
            assets,
            srcsets,
            root: root.to_path_buf(),
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

    /// Fingerprint of the responsive width-variant manifest, for the build cache.
    pub fn srcsets(&self) -> crate::graph::Hash {
        self.srcsets.fingerprint()
    }

    /// Run the DOM transform pipeline over a page's document in place: link
    /// resolution (source-path `.typ` links to permalinks) first, then the
    /// configured transforms. Returns the raw targets of any internal `.typ`
    /// links that point at a non-existent page.
    pub fn rewrite(&self, doc: &mut HtmlDocument, page: &Page, config: &Config) -> Rewrite {
        let mut cx = Cx {
            config,
            page,
            links: &self.links,
            assets: &self.assets,
            srcsets: &self.srcsets,
            root: &self.root,
            broken: Vec::new(),
            images: Vec::new(),
        };
        self.transforms.apply(doc, &mut cx);
        Rewrite {
            broken: cx.broken,
            images: cx.images,
        }
    }
}
