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
mod inline;
mod links;
mod lint;
mod origin;
mod scope;
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
    /// The entity registries every page's references resolve against, so a
    /// byline is drawn from the same roster the plan validated.
    entities: crate::content::Registries,
    assets: AssetMap,
    srcsets: SrcSets,
    /// What this build emitted, so a reference can be stamped with the digest
    /// of the file it names.
    emitted: Emitted,
    /// The content tree as the compiler spells it (project-relative), resolved
    /// once here because every page's links are tested against it and the answer
    /// is the same for the whole build.
    content: std::path::PathBuf,
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
    /// The URLs this page's already-URL links named, and whether the site served
    /// a page at each. Its dependency on the *page set* rather than on the URL
    /// layout, which is what [`Rewrite::links`] records.
    pub urls: UrlDeps,
    /// The variant-manifest entries this page's images consulted, its
    /// dependency on the responsive pipeline.
    pub srcsets: SrcSetDeps,
    /// The asset-map entries this page's references consulted, its dependency
    /// on the processed-asset tree.
    pub assets: AssetDeps,
    /// The pages this page's *content* links to: its edges of the site's link
    /// graph, which the backlink pass inverts. Template chrome is left out; see
    /// [`Outbound`].
    pub outbound: Outbound,
    /// Images lifted out of the DOM, for the engine to copy into `dist`.
    pub images: Vec<ImageRef>,
    /// The assets the build provides itself that this page asked for, by the
    /// path each is known by under the asset root (`math.css`).
    ///
    /// The pipeline names and digests every one of them before a page renders,
    /// but cannot know which are wanted until the pages exist, so it writes none
    /// of them. This is that answer, and it travels with the page for the same
    /// reason [`images`](Self::images) does: the asset tree is rebuilt every
    /// build, so a page served from cache still has to keep its file alive.
    pub owned: std::collections::BTreeSet<String>,
    /// Outbound `http(s)` link targets the page carries, collected only when
    /// external checking is on.
    pub external: Vec<String>,
    /// The heading ids this page exposes, so a link elsewhere can be checked
    /// against them.
    pub anchors: Vec<String>,
    /// The links this page carries that name a `#fragment`, whichever page's:
    /// a resolved link into another page, and a bare `#section` into this one.
    ///
    /// Collected rather than checked on the spot: even for a link into its own
    /// body, a page's ids are not known while its links are being resolved (the
    /// anchor pass runs after), and a target page's are not known at all (pages
    /// render in parallel). So the check is a site-wide pass once every page
    /// has produced its set.
    pub deep: Vec<Target>,
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
    /// The digests of the page's inline scripts and styles, for the generated
    /// content security policy. Empty unless one is being generated.
    pub inline: Inline,
    /// Markers the render pass refused: an icon `svg()` could not turn into DOM
    /// nodes, an image marker naming a path outside the project. The element is
    /// already in the page in either case, so the caller must fail rather than
    /// ship an empty `<svg>` or a `src` naming a file nothing wrote.
    ///
    /// Typed as the crate's error rather than as one pass's, because the two
    /// passes that write it report different classes and a third would report a
    /// third: the channel is "this page cannot be shipped", not "an SVG failed".
    pub invalid: Vec<crate::error::BaudelaireErrorKind>,
}

/// The live render-side maps a page's recorded probes are revalidated against.
///
/// Grouped because they travel together and answer one question: given what a
/// page consulted while rendering, may its markup be reused? Passing them as
/// one value also keeps the fact that they come from a single renderer, rather
/// than three unrelated arguments a caller could pair up wrongly.
#[derive(Clone, Copy)]
pub struct RenderMaps<'a> {
    pub links: &'a LinkMap,
    pub srcsets: &'a SrcSets,
    pub assets: &'a AssetMap,
}

/// What a renderer is built from: everything site-wide a page is rendered
/// against.
///
/// A parameter object rather than a row of positional arguments, the same shape
/// [`crate::graph::Recorded`] takes and for the same reason: a new site-wide
/// input is one field here and one line at the call site, instead of a wider
/// signature every caller has to re-spell in the right order.
pub struct Inputs<'a> {
    /// Every page the build will render, for the link map.
    pub pages: &'a [Page],
    /// The entity registries a page's credited references resolve against.
    pub entities: crate::content::Registries,
    /// The processed-asset URL map every reference is rewritten through.
    pub assets: AssetMap,
    /// The responsive width variants each image matched.
    pub srcsets: SrcSets,
    /// What this build emitted, so a reference can be stamped with a digest.
    pub emitted: Emitted,
    /// The typst project root absolute link paths resolve against.
    pub root: &'a std::path::Path,
    /// The content tree as the compiler spells it.
    pub content: std::path::PathBuf,
    /// The extensions a page may be written in, for the link map.
    pub sources: Vec<&'static str>,
}

impl Renderer {
    /// Build a renderer over one build's site-wide inputs.
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
        };
        // Before the transforms, because three of them append to the head and
        // each would otherwise find nothing and quietly do nothing. typst-html
        // owns the document root, so a page with no `<head>` is a page whose own
        // markup replaced it; the charset and the title went with it, and no
        // later pass can put them back.
        if doc.head().is_none() {
            // Spelled as the project spells it: `page.source` is absolute, and a
            // diagnostic naming a build directory is one the author cannot match
            // against anything they wrote.
            let named = page.source.strip_prefix(&self.root).unwrap_or(&page.source);
            cx.found
                .invalid
                .push(crate::error::TemplateOwnsRoot::new(named.display()).into());
        }
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
