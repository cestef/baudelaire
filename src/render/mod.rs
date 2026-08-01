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
mod emitted;
mod fragment;
mod links;
mod lint;
mod origin;
mod scope;
mod srcset;
mod transform;

pub use asset::{AssetDeps, AssetMap};
pub use emitted::Emitted;
pub use fragment::Fragments;
pub use links::{LinkDeps, LinkMap};
pub use lint::{Finding, Load, Reference, Weight};
pub use origin::Site;
pub use srcset::{SrcSetDeps, SrcSets};
pub use transform::ImageRef;

use typst_html::HtmlDocument;

use crate::config::Config;
use crate::content::Page;

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
    /// The lint rules, run over the finished DOM once every transform has had
    /// its say: a rule judges the page as it will be served, not as typst first
    /// emitted it.
    lints: lint::Rules,
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
    /// The variant-manifest entries this page's images consulted, its
    /// dependency on the responsive pipeline.
    pub srcsets: SrcSetDeps,
    /// The asset-map entries this page's references consulted, its dependency
    /// on the processed-asset tree.
    pub assets: AssetDeps,
    /// Images lifted out of the DOM, for the engine to copy into `dist`.
    pub images: Vec<ImageRef>,
    /// Outbound `http(s)` link targets the page carries, collected only when
    /// external checking is on.
    pub external: Vec<String>,
    /// The heading ids this page exposes, so a link elsewhere can be checked
    /// against them.
    pub anchors: Vec<String>,
    /// Resolved links this page carries that name a fragment of *another* page,
    /// as full `"/url/#fragment"` targets.
    ///
    /// Collected rather than checked on the spot: the target page's anchors are
    /// not known while this one renders (pages render in parallel, and the
    /// anchor pass runs after link resolution within each), so the check is a
    /// site-wide pass once every page has produced its set.
    pub deep: Vec<String>,
    /// Files the render pass read on this page's behalf, to add to its
    /// dependencies: baudelaire reads them, not typst, so nothing else would
    /// notice an edit. Inlined SVG icons and embedded assets both land here,
    /// which is why they need no cache mechanism of their own.
    pub read: Vec<std::path::PathBuf>,
    /// What the lint pass found on this page, empty unless `lint { }` is on.
    /// Recorded rather than reported here: the pass runs inside a rayon map
    /// over the pages, and a finding is one line of a single site-wide report.
    pub lints: Vec<Finding>,
    /// What the page ships, for the budget check. Recorded for the same reason
    /// as `lints`, and resolved to bytes site-wide, where the sizes of the
    /// files it names are known.
    pub weight: Weight,
    /// SVG files `svg()` marked that could not be turned into DOM nodes. The
    /// element is already in the page, so the caller must fail rather than ship
    /// an empty `<svg>` where an icon was asked for.
    pub invalid: Vec<crate::error::SvgError>,
}

/// The live render-side maps a page's recorded probes are revalidated against.
///
/// Grouped because they travel together and answer one question: given what a
/// page consulted while rendering, may its markup be reused? Passing them as
/// one value also keeps the fact that they come from a single renderer, rather
/// than three unrelated arguments a caller could pair up wrongly.
pub struct RenderMaps<'a> {
    pub links: &'a LinkMap,
    pub srcsets: &'a SrcSets,
    pub assets: &'a AssetMap,
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
            lints: lint::Rules::builtin(),
        }
    }

    /// The site-wide maps every page's probes are checked against, for the
    /// build cache.
    pub fn maps(&self) -> RenderMaps<'_> {
        RenderMaps {
            links: &self.links,
            srcsets: &self.srcsets,
            assets: &self.assets,
        }
    }

    /// Run the DOM transform pipeline over a page's document in place: link
    /// resolution (source-path `.typ` links to permalinks) first, then the
    /// configured transforms. `world` is the one the page compiled in, so a
    /// transform can resolve the spans its nodes carry. Returns the raw targets
    /// of any internal `.typ` links that point at a non-existent page.
    pub fn rewrite(
        &self,
        doc: &mut HtmlDocument,
        page: &Page,
        config: &Config,
        world: &crate::world::PageWorld,
    ) -> Rewrite {
        let mut cx = Cx {
            config,
            page,
            links: &self.links,
            assets: &self.assets,
            srcsets: &self.srcsets,
            root: &self.root,
            world,
            found: Rewrite::default(),
        };
        self.transforms.apply(doc, &mut cx);
        // After the transforms, deliberately: a rule judges the markup as it
        // will be served, footnotes moved and icons inlined, and a page whose
        // budget the pipeline blew has no way to say so from the DOM typst
        // first handed over.
        if config.lint.enabled {
            let (lints, weight) = self.lints.run(doc, &config.lint, world);
            cx.found.lints = lints;
            cx.found.weight = weight;
        }
        cx.found
    }
}
