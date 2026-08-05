//! `generate { pdf { } }`: a PDF per page, and bundled documents.

use crate::config::Basename;
use crate::config::dispatch::Kind::{Block as Nested, Flag, Text, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// What the typesetter writes on paper: `generate { pdf { .. } }`.
///
/// The other half of what it can do with the same source: the HTML compile
/// targets a DOM, these target pages. Two artifacts, switched on separately by
/// the presence of their own block, because wanting one says nothing about
/// wanting the other: a manual is bundled and rarely per-page, a blog is the
/// reverse.
#[derive(Debug, Clone, Hash, Default)]
pub struct PdfConfig {
    /// One PDF per page, beside its HTML.
    pub pages: PdfPages,
    /// Many pages as one document.
    pub bundle: PdfBundle,
}

impl PdfConfig {
    /// Whether the site asked for either artifact, for the feature gate: a
    /// binary without the exporter has to say so whichever one was asked for.
    pub fn enabled(&self) -> bool {
        self.pages.enabled || self.bundle.enabled()
    }
}

/// One PDF per page, from a paged template. Enabled by the presence of a
/// `generate { pdf { pages { .. } } }` block.
///
/// Like a card it needs its own template, because a layout that emits
/// `html.elem` produces nothing on the paged target.
#[derive(Debug, Clone, Hash)]
pub struct PdfPages {
    /// Whether to write a PDF per page.
    pub enabled: bool,
    /// The paged template file under the templates directory.
    pub template: String,
}

impl PdfPages {
    /// The served URL of a page's PDF: a sibling of the page rather than a file
    /// inside it, so `/posts/hello/` yields `/posts/hello.pdf` and a browser
    /// saves it under a name that means something. `/posts/hello/index.pdf`
    /// would download as `index.pdf`.
    pub fn url(&self, permalink: &str) -> String {
        format!("/{}.pdf", Basename(permalink))
    }

    /// Whether per-page PDFs are actually produced: configured *and* compiled
    /// in. A build without the `pdf` feature has no exporter, so linking pages
    /// to a file it cannot make would be worse than making none.
    pub fn active(&self) -> bool {
        self.enabled && cfg!(feature = "pdf")
    }
}

/// Many pages as one document: a collection bound end to end, the whole site,
/// or both. Enabled by the presence of a `generate { pdf { bundle { .. } } }`
/// block naming at least one target.
///
/// The paged sibling of `navigation { standalone }`, which does the same thing
/// for HTML.
#[derive(Debug, Clone, Hash)]
pub struct PdfBundle {
    /// Whether the site wrote a `bundle { }` block at all, as distinct from
    /// having named a target in one. An empty block asks for nothing, and the
    /// difference is what lets the build say so instead of writing no file in
    /// silence.
    pub present: bool,
    /// The paged template file under the templates directory. Distinct from the
    /// per-page one: it is handed every page at once, and what it does with a
    /// run of documents (a title page, a contents list, running heads) is not
    /// what a single page's template does.
    pub template: String,
    /// Collections to bundle, each written to `/<collection>.pdf`.
    pub collections: Vec<String>,
    /// Whether to bundle the whole site, written to `/site.pdf`. Named by the
    /// same rule its neighbours are: a bundle is `/<target>.pdf`, and inventing
    /// a second rule so this one could carry a filename bought nothing.
    pub site: bool,
}

impl PdfBundle {
    /// Whether any target was named. A `bundle { }` block that names none asks
    /// for nothing, which [`crate::engine`]'s inert-setting table reports
    /// rather than letting the build write nothing in silence.
    pub fn enabled(&self) -> bool {
        !self.collections.is_empty() || self.site
    }

    /// Whether bundles are actually produced: asked for *and* compiled in.
    pub fn active(&self) -> bool {
        self.enabled() && cfg!(feature = "pdf")
    }
}

impl Default for PdfPages {
    fn default() -> Self {
        // opt-in for the same reason a card is: it is a second compile of every
        // page, and this one lays the whole document out rather than one page.
        Self {
            enabled: false,
            template: "print.typ".into(),
        }
    }
}

impl Default for PdfBundle {
    fn default() -> Self {
        // No target: a bundle binds what the block names, and naming nothing
        // is asking for nothing. The template is the value a `bundle { }` block
        // inherits when it stays silent about it.
        Self {
            present: false,
            template: "book.typ".into(),
            collections: Vec::new(),
            site: false,
        }
    }
}

/// The `pdf { .. }` section: what the typesetter writes on paper. Each child
/// block's presence enables that artifact.
impl Section for PdfConfig {
    const RULES: Block<Self> = Block(&[
        (
            "pages",
            Nested(PdfPages::rows),
            "A PDF per page, beside its HTML. Its presence turns it on.",
            |c, n, t| c.pages.fill(n, t),
        ),
        (
            "bundle",
            Nested(PdfBundle::rows),
            "Many pages as one document. Needs a target named below.",
            |c, n, t| c.bundle.fill(n, t),
        ),
    ]);
}

/// The `pages { template .. }` block. Its presence enables the per-page PDF.
impl Section for PdfPages {
    const RULES: Block<Self> = Block(&[(
        "template",
        Text,
        "The typst template each page is typeset with.",
        |c, n, t| {
            c.template = n.string(t, 0)?;
            Ok(())
        },
    )]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

/// The `bundle { template ..; collections ..; site .. }` block: many pages as
/// one document. Unlike its neighbours, presence alone enables nothing: a
/// bundle needs a target to bind.
impl Section for PdfBundle {
    const RULES: Block<Self> = Block(&[
        (
            "template",
            Text,
            "The typst template the bundle is typeset with.",
            |c, n, t| {
                c.template = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "collections",
            Texts,
            "Which collections the bundle gathers, one word each.",
            |c, n, t| {
                c.collections = n.words(t)?;
                Ok(())
            },
        ),
        (
            "site",
            Flag,
            "Bundle the whole site rather than named collections.",
            |c, n, t| {
                c.site = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);

    /// Presence is recorded but enables nothing: a bundle binds what the block
    /// names, and naming nothing is asking for nothing.
    fn enable(&mut self) -> bool {
        self.present = true;
        true
    }
}
