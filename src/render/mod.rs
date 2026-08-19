//! Render layer: post-processes compiled documents on typst-html's typed DOM.
//!
//! The site-wide data a transform reads lives here; the passes themselves live
//! in [`transform`], one file each.

mod asset;
mod emitted;
mod fragment;
mod inline;
mod links;
pub mod lint;
mod origin;
mod scope;
pub mod snippet;
mod srcset;
mod transform;

pub use asset::{AssetDeps, AssetMap};
pub use emitted::{Emission, Emitted};
pub use fragment::{Fragments, Syndicated};
pub use inline::Inline;
pub use links::{Backlink, Backlinks, LinkDeps, LinkMap, Outbound, Target, UrlDeps};
pub use lint::{Finding, Load, Reference, Weight};
pub use origin::Site;
pub use srcset::{Candidate, SrcSetDeps, SrcSets};
pub use transform::ImageRef;

use typst_html::HtmlDocument;

use crate::config::Config;
use crate::content::Page;

use crate::render::transform::{Cx, DocumentExt, Transforms};

/// A raw `href`/`src` split at its `#fragment` / `?query` boundary.
pub(crate) struct Tail<'a> {
    pub path: &'a str,
    /// The trailing `#fragment` / `?query`, empty when absent.
    pub tail: &'a str,
}

impl<'a> Tail<'a> {
    pub fn of(raw: &'a str) -> Self {
        let (path, tail) = raw.find(['#', '?']).map_or((raw, ""), |i| raw.split_at(i));
        Self { path, tail }
    }
}

/// The site-wide render context, built once per build and shared read-only
/// across the parallel compile pool.
pub struct Renderer {
    links: LinkMap,
    entities: crate::content::Registries,
    assets: AssetMap,
    srcsets: SrcSets,
    emitted: Emitted,
    /// The content tree as the compiler spells it, project-relative.
    content: std::path::PathBuf,
    root: std::path::PathBuf,
    transforms: Transforms,
    lints: lint::Rules,
}

/// The findings of running the transform pipeline over one page.
#[derive(Default)]
pub struct Rewrite {
    /// Raw targets of internal `.typ` links that point at a non-existent page.
    pub broken: Vec<String>,
    /// The link-map entries this page's links resolved against, keyed by
    /// canonical source path.
    pub links: LinkDeps,
    /// The URLs this page's already-URL links named, and whether the site
    /// served a page at each.
    pub urls: UrlDeps,
    /// The variant-manifest entries this page's images consulted.
    pub srcsets: SrcSetDeps,
    /// The asset-map entries this page's references consulted.
    pub assets: AssetDeps,
    /// The pages this page's *content* links to, the edges the backlink pass
    /// inverts.
    pub outbound: Outbound,
    /// Images lifted out of the DOM, for the engine to copy into `dist`.
    pub images: Vec<ImageRef>,
    /// The assets the build provides itself that this page asked for, by the
    /// path each is known by under the asset root (`math.css`).
    pub owned: std::collections::BTreeSet<String>,
    /// Outbound `http(s)` link targets the page carries, collected only when
    /// external checking is on.
    pub external: Vec<String>,
    /// The heading ids this page exposes, so a link elsewhere can be checked
    /// against them.
    pub anchors: Vec<String>,
    /// The links this page carries that name a `#fragment`, whichever page's:
    /// a resolved link into another page, and a bare `#section` into this one.
    pub deep: Vec<Target>,
    /// Files the render pass read on this page's behalf, to add to its
    /// dependencies: baudelaire reads them, not typst, so nothing else would
    /// notice an edit.
    pub read: Vec<std::path::PathBuf>,
    /// What the lint pass found on this page, empty unless `check { }` is on.
    pub lints: Vec<Finding>,
    /// What the page ships, for the budget check.
    pub weight: Weight,
    /// The digests of the page's inline scripts and styles, for the generated
    /// content security policy. Empty unless one is being generated.
    pub inline: Inline,
    /// Markers the render pass refused: an icon `svg()` could not turn into DOM
    /// nodes, an image marker naming a path outside the project. The element is
    /// already in the page in either case, so the caller must fail rather than
    /// ship an empty `<svg>` or a `src` naming a file nothing wrote.
    pub invalid: Vec<crate::error::BaudelaireErrorKind>,
}

/// The live render-side maps a page's recorded probes are revalidated against.
#[derive(Clone, Copy)]
pub struct RenderMaps<'a> {
    pub links: &'a LinkMap,
    pub srcsets: &'a SrcSets,
    pub assets: &'a AssetMap,
}

/// What a renderer is built from: everything site-wide a page is rendered
/// against.
pub struct Inputs<'a> {
    /// Every page the build will render, for the link map.
    pub pages: &'a [Page],
    pub entities: crate::content::Registries,
    /// The processed-asset URL map every reference is rewritten through.
    pub assets: AssetMap,
    /// The responsive width variants each image matched.
    pub srcsets: SrcSets,
    pub emitted: Emitted,
    /// The typst project root absolute link paths resolve against.
    pub root: &'a std::path::Path,
    /// The content tree as the compiler spells it.
    pub content: std::path::PathBuf,
    /// The extensions a page may be written in, for the link map.
    pub sources: Vec<&'static str>,
}

impl Renderer {
    pub fn new(inputs: Inputs<'_>) -> Self {
        let Inputs {
            pages,
            entities,
            assets,
            srcsets,
            emitted,
            root,
            content,
            sources,
        } = inputs;
        Self {
            links: LinkMap::new(pages, root, sources),
            entities,
            content,
            assets,
            srcsets,
            emitted,
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

    /// Run the DOM transform pipeline over a page's document in place, then the
    /// lint rules over the markup as it will be served. `world` is the one the
    /// page compiled in, so a transform can resolve the spans its nodes carry.
    pub fn rewrite(
        &self,
        doc: &mut HtmlDocument,
        page: &Page,
        related: &crate::content::Related,
        config: &Config,
        world: &crate::world::PageWorld,
    ) -> Rewrite {
        let mut cx = Cx {
            config,
            page,
            related,
            entities: &self.entities,
            links: &self.links,
            assets: &self.assets,
            srcsets: &self.srcsets,
            emitted: &self.emitted,
            root: &self.root,
            content: &self.content,
            world,
            found: Rewrite::default(),
            extracted: std::collections::BTreeMap::new(),
            fences: Vec::new(),
            exempt: lint::Exemptions::default(),
        };
        if doc.head().is_none() {
            let named = page.source.strip_prefix(&self.root).unwrap_or(&page.source);
            cx.found
                .invalid
                .push(crate::error::TemplateOwnsRoot::new(named.display()).into());
        }
        self.transforms.apply(doc, &mut cx);
        if config.check.enabled {
            let (lints, weight) = self.lints.run(
                doc,
                &config.check,
                world,
                &self.root,
                &cx.fences,
                &cx.exempt,
            );
            cx.found.lints = lints;
            cx.found.weight = weight;
        }
        cx.found
    }
}
