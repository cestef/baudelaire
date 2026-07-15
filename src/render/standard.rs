//! Injects the per-page standard.site verification `<link>` into dated pages.
//!
//! When `publish.standard` carries a `did` and `verify.links` is on, every dated
//! page gets `<link rel="site.standard.document" href="at://..">` in its `<head>`,
//! letting an AppView confirm the page and its record belong together. The URI —
//! and the key scheme behind it — comes from [`crate::publish::standard`], the
//! single source of the record shapes, so the build names exactly what the
//! publisher writes.

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::Config;
use crate::publish::standard::{DOCUMENT, document_uri};

use super::transform::{Cx, ElementExt, Transform};

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
        if let Some(head) = doc.root_mut().head() {
            head.children.push(link(&href));
        }
    }
}

/// A `<link rel="site.standard.document" href="..">` node.
fn link(href: &str) -> HtmlNode {
    let mut el = HtmlElement::new(tag::link);
    el.attrs.push(attr::rel, DOCUMENT.as_str());
    el.attrs.push(attr::href, href);
    HtmlNode::Element(el)
}
