//! Stamps each element with `data-typst="file:line:column"`, the source
//! location it was compiled from, which the live preview reads to jump back.

use typst_html::{HtmlAttr, HtmlDocument};

use crate::config::Config;
use crate::content::Rebased;
use crate::render::origin::Origins;

use super::{Cx, DocumentExt, ElementExt, Transform};

/// The attribute a stamped element carries, short enough for
/// [`HtmlAttr::constant`]'s inline representation.
const SOURCE_ATTR: HtmlAttr = HtmlAttr::constant("data-typst");

/// The [`Transform`] that stamps source locations onto the DOM.
pub(super) struct Spans;

impl Spans {
    /// This page's map back to the language its author wrote it in, positioned
    /// in the wrapper that was compiled.
    ///
    /// `None` for a `.typ` page, whose spans already name the file its author
    /// opened.
    #[cfg(feature = "markdown")]
    fn lowered(cx: &Cx<'_>) -> Option<Rebased> {
        let crate::content::Data::Lowered { sourcemap, .. } = &cx.page.data else {
            return None;
        };
        Rebased::new(std::sync::Arc::clone(sourcemap), cx.world.source().text())
    }

    /// Without the markdown feature no page is lowered, so every span already
    /// names a file its author wrote.
    #[cfg(not(feature = "markdown"))]
    fn lowered(_: &Cx<'_>) -> Option<Rebased> {
        None
    }
}

impl Transform for Spans {
    fn enabled(&self, config: &Config) -> bool {
        config.html.spans
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut origins = Origins::new(cx.world);
        let lowered = Self::lowered(cx);
        doc.walk(|element| {
            let origin = match &lowered {
                Some(map) => origins.mapped(element.span, map),
                None => origins.locate(element.span),
            };
            if let Some(origin) = origin {
                element.set(SOURCE_ATTR, &origin.to_string());
            }
        });
    }
}
