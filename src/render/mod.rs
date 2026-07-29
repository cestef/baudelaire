//! Render layer: post-processes compiled documents before serialization.
//!
//! Post-processing operates on typst-html's own typed DOM
//! ([`typst_html::HtmlDocument`]), never on the serialized string, honoring the
//! project rule that HTML is never manipulated as text.
//!
//! The site-wide data a transform reads (the asset map, the link map, the
//! responsive manifest) lives here; the passes themselves live in
//! [`transform`], one file each.

mod asset;
mod fragment;
mod links;
mod scope;
mod srcset;
mod transform;

pub use asset::AssetMap;
pub use fragment::Fragments;
pub use links::{LinkDeps, LinkMap};
pub use srcset::SrcSets;
pub use transform::ImageRef;

use typst_html::HtmlDocument;

use crate::config::Config;
use crate::content::Page;
use crate::graph::Fingerprint;
use crate::render::transform::{Cx, Transforms};

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

/// The findings of running the transform pipeline over one page.
///
/// Transforms accumulate into this directly (as [`transform::Cx::found`]), so
/// adding a finding is one field here rather than one field in two places and a
/// copy between them.
#[derive(Default)]
pub struct Rewrite {
    /// Raw targets of internal `.typ` links that point at a non-existent page.
    pub broken: Vec<String>,
    /// The link-map entries this page's links resolved against, its dependency
    /// on the site's URL layout. Keyed by canonical source path; the cache
    /// stores them the way it stores every other path.
    pub links: LinkDeps,
    /// Images lifted out of the DOM, for the engine to copy into `dist`.
    pub images: Vec<ImageRef>,
    /// Outbound `http(s)` link targets the page carries, collected only when
    /// external checking is on.
    pub external: Vec<String>,
    /// Icon files the svg transform inlined, to add to the page's dependencies:
    /// baudelaire reads them, not typst, so nothing else would notice an edit.
    pub icons: Vec<std::path::PathBuf>,
    /// SVG files `svg()` marked that could not be turned into DOM nodes. The
    /// element is already in the page, so the caller must fail rather than ship
    /// an empty `<svg>` where an icon was asked for.
    pub invalid: Vec<crate::error::SvgError>,
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

    /// The page-to-permalink map, for the build cache to revalidate each page's
    /// recorded [`LinkDeps`] against.
    pub fn links(&self) -> &LinkMap {
        &self.links
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
            found: Rewrite::default(),
        };
        self.transforms.apply(doc, &mut cx);
        cx.found
    }
}
