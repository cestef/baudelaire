//! Injects the per-page standard.site verification `<link>` into dated pages,
//! so an AppView can confirm the page and its record belong together.

use typst_html::{HtmlDocument, HtmlElement, attr, tag};

use crate::announce::standard::{DOCUMENT, document_uri};
use crate::config::Config;

use super::{Cx, DocumentExt, Transform};

/// The transform that adds each dated page's `site.standard.document` backlink;
/// only dated pages are documents.
pub(super) struct Verify;

impl Transform for Verify {
    fn enabled(&self, config: &Config) -> bool {
        config.verify_did(|v| v.links).is_some()
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let (Some(did), true) = (
            cx.config.verify_did(|v| v.links),
            cx.page.frontmatter.date.is_some(),
        ) else {
            return;
        };
        let href = document_uri(did, &cx.page.permalink).to_string();
        if let Some(head) = doc.head() {
            head.children.push(
                HtmlElement::new(tag::link)
                    .with_attr(attr::rel, DOCUMENT.as_str())
                    .with_attr(attr::href, href)
                    .into(),
            );
        }
    }
}
