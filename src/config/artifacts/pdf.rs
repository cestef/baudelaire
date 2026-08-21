//! `artifacts { pdf { } }`: a PDF per page. A document bound from *many* pages
//! is `artifacts { bundles { } }`.

use dispatch_derive::Table;

use crate::config::Basename;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// What the typesetter writes on paper, page by page:
/// `artifacts { pdf { .. } }`.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct PdfConfig {
    /// A PDF per page, beside its HTML. Its presence turns it on; `#false` turns it off again.
    #[key(nested(PdfPages))]
    pub pages: PdfPages,
}

impl PdfConfig {
    /// Whether the site asked for a per-page PDF, for the feature gate.
    pub fn enabled(&self) -> bool {
        self.pages.enabled
    }
}

/// One PDF per page, enabled by the presence of a
/// `artifacts { pdf { pages { .. } } }` block.
///
/// Like a card it needs its own template, because a layout that emits
/// `html.elem` produces nothing on the paged target.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct PdfPages {
    pub enabled: bool,

    /// The typst template each page is typeset with.
    ///
    /// A paged template file under the templates directory.
    #[key(text)]
    pub template: String,
}

impl PdfPages {
    /// The served URL of a page's PDF: a sibling of the page rather than a file
    /// inside it, so `/posts/hello/` yields `/posts/hello.pdf` and a browser
    /// saves it under a name that means something.
    pub fn url(&self, permalink: &str) -> String {
        format!("/{}.pdf", Basename(permalink))
    }

    /// Whether per-page PDFs are actually produced: configured *and* compiled
    /// in.
    pub fn active(&self) -> bool {
        self.enabled && cfg!(feature = "pdf")
    }
}

impl Default for PdfPages {
    fn default() -> Self {
        Self {
            enabled: false,
            template: "print.typ".into(),
        }
    }
}
