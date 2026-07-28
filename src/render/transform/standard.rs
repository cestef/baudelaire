//! Injects the per-page standard.site verification `<link>` into dated pages.
//!
//! When `publish.standard` carries a `did` and `verify.links` is on, every dated
//! page gets `<link rel="site.standard.document" href="at://..">` in its `<head>`,
//! letting an AppView confirm the page and its record belong together. The URI
//! (and the key scheme behind it) comes from [`crate::announce::standard`], the
//! single source of the record shapes, so the build names exactly what the
//! publisher writes.

use typst_html::{HtmlDocument, HtmlElement, attr, tag};

use crate::announce::standard::{DOCUMENT, document_uri};
use crate::config::Config;

use super::{Cx, DocumentExt, Transform};

/// The transform that adds each dated page's `site.standard.document` backlink.
pub(super) struct Verify;

impl Transform for Verify {
    fn enabled(&self, config: &Config) -> bool {
        config.verify_did(|v| v.links).is_some()
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        // only dated pages are documents; the did gate matches the publisher
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
