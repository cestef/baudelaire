//! Stamps each element with the source location it was compiled from.
//!
//! Typst carries a span on every node it emits, and the typed DOM hands it
//! straight through, so a rendered element can say which `.typ` file, line, and
//! column produced it. With `html { spans #true }` each one gets a
//! `data-typst="file:line:column"`, which is what the live preview reads to
//! jump from something on screen back to the source that wrote it. Resolving a
//! span is [`crate::render::origin`]'s job, shared with the lint pass.
//!
//! Off in a published build: see [`crate::config::HtmlConfig::spans`].

use typst_html::{HtmlAttr, HtmlDocument};

use crate::config::Config;
use crate::render::origin::Origins;

use super::{Cx, DocumentExt, ElementExt, Transform};

/// The attribute a stamped element carries. Short enough for
/// [`HtmlAttr::constant`]'s inline representation, so a name this file can
/// never spell wrongly at runtime.
const SOURCE_ATTR: HtmlAttr = HtmlAttr::constant("data-typst");

/// The [`Transform`] that stamps source locations onto the DOM.
pub(super) struct Spans;

impl Transform for Spans {
    fn enabled(&self, config: &Config) -> bool {
        config.html.spans
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut origins = Origins::new(cx.world);
        doc.walk(|element| {
            if let Some(origin) = origins.locate(element.span) {
                element.set(SOURCE_ATTR, &origin.to_string());
            }
        });
    }
}
