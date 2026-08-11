//! `generate { pdf { } }`: a PDF per page.
//!
//! A document bound from *many* pages is `generate { bundles { } }`: it is a
//! selection of pages that happens to be written as a PDF, and the same
//! selection is written as an EPUB by naming one more format.

use crate::config::Basename;
use crate::config::dispatch::Kind::{Block as Nested, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// What the typesetter writes on paper, page by page:
/// `generate { pdf { .. } }`.
///
/// The other half of what it can do with the same source: the HTML compile
/// targets a DOM, this targets pages.
#[derive(Debug, Clone, Hash, Default)]
pub struct PdfConfig {
    /// One PDF per page, beside its HTML.
    pub pages: PdfPages,
}

impl PdfConfig {
    /// Whether the site asked for a per-page PDF, for the feature gate. A
    /// bound document is `generate { bundles }`' now, and carries its own row:
    /// it is a selection of pages that happens to be written as a PDF, not a
    /// second thing this block does.
    pub fn enabled(&self) -> bool {
        self.pages.enabled
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

impl Section for PdfConfig {
    const RULES: Block<Self> = Block(&[(
        "pages",
        Nested(PdfPages::rows),
        "A PDF per page, beside its HTML. Its presence turns it on; `#false` turns it off again.",
        |c, n, t| c.pages.fill(n, t),
    )]);
}

/// The `pages { template .. }` block. Its presence enables the per-page PDF.
impl Section for PdfPages {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[(
        "template",
        Text,
        "The typst template each page is typeset with.",
        |c, n, t| {
            c.template = n.string(t, 0)?;
            Ok(())
        },
    )]);
}
