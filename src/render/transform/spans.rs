//! Stamps each element with the source location it was compiled from.
//!
//! Typst carries a span on every node it emits, and the typed DOM hands it
//! straight through, so a rendered element can say which file, line, and column
//! produced it. With `html { spans #true }` each one gets a
//! `data-typst="file:line:column"`, which is what the live preview reads to
//! jump from something on screen back to the source that wrote it. Resolving a
//! span is [`crate::render::origin`]'s job, shared with the lint pass.
//!
//! A page this crate lowered needs a second hop. Its Typst is generated, under
//! a wrapper path that names no file, so the compiler's own location is the one
//! thing an editor cannot open: the stamps read `content/a.md@layout:3:2`
//! against a page whose author wrote markdown. [`crate::content::SourceMap`]
//! carries them the rest of the way, to the `.md` line and column.
//!
//! Off in a published build: see [`crate::config::HtmlConfig::spans`].

use typst_html::{HtmlAttr, HtmlDocument};

use crate::config::Config;
use crate::content::Rebased;
use crate::render::origin::Origins;

use super::{Cx, DocumentExt, ElementExt, Transform};

/// The attribute a stamped element carries. Short enough for
/// [`HtmlAttr::constant`]'s inline representation, so a name this file can
/// never spell wrongly at runtime.
const SOURCE_ATTR: HtmlAttr = HtmlAttr::constant("data-typst");

/// The [`Transform`] that stamps source locations onto the DOM.
pub(super) struct Spans;

impl Spans {
    /// This page's map back to the language its author wrote it in, positioned
    /// in the wrapper that was compiled.
    ///
    /// `None` for a `.typ` page, whose spans already name the file its author
    /// opened, and which must keep stamping exactly what it stamps today.
    #[cfg(feature = "markdown")]
    fn lowered(cx: &Cx<'_>) -> Option<Rebased> {
        let crate::content::Data::Lowered { sourcemap, .. } = &cx.page.data else {
            return None;
        };
        Rebased::new(std::sync::Arc::clone(sourcemap), cx.world.source().text())
    }

    /// Without the markdown feature no page is lowered from anything, so every
    /// span already names a file its author wrote.
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
