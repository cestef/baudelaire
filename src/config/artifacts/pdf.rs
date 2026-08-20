//! `artifacts { pdf { } }`: a PDF per page. A document bound from *many* pages
//! is `artifacts { bundles { } }`.

use crate::config::Basename;
use crate::config::dispatch::Kind::{Block as Nested, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// What the typesetter writes on paper, page by page:
/// `artifacts { pdf { .. } }`.
#[derive(Debug, Clone, Hash, Default)]
pub struct PdfConfig {
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
#[derive(Debug, Clone, Hash)]
pub struct PdfPages {
    pub enabled: bool,
    /// The paged template file under the templates directory.
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

impl Section for PdfConfig {
    const RULES: Block<Self> = Block(&[(
        "pages",
        Nested(PdfPages::rows),
        "A PDF per page, beside its HTML. Its presence turns it on; `#false` turns it off again.",
        |c| c.pages.values(),
        |c, n, t| c.pages.fill(n, t),
    )]);
}

/// The `pages { }` block, whose presence enables the per-page PDF.
impl Section for PdfPages {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[(
        "template",
        Text,
        "The typst template each page is typeset with.",
        |c| c.template.clone().into(),
        |c, n, t| {
            c.template = n.string(t, 0)?;
            Ok(())
        },
    )]);
}
