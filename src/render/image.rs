//! Improves `<img>` loading behaviour.
//!
//! When `html { images true }` is set (the default), every image gains
//! `loading="lazy"` (defer offscreen images) and `decoding="async"` (never block
//! rendering on decode) unless the author already set them. Best-effort and
//! attribute-only — the image bytes are untouched.

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::Config;

use super::transform::{Cx, Transform};

/// The [`Transform`] that annotates images for lazy, async loading.
pub(super) struct Images;

impl Transform for Images {
    fn enabled(&self, config: &Config) -> bool {
        config.images.lazy
    }

    fn apply(&self, doc: &mut HtmlDocument, _cx: &mut Cx<'_>) {
        Self::visit(doc.root_mut());
    }
}

impl Images {
    fn visit(element: &mut HtmlElement) {
        if element.tag == tag::img {
            // Only fill in what the author left unspecified.
            if element.attrs.get(attr::loading).is_none() {
                element.attrs.push(attr::loading, "lazy");
            }
            if element.attrs.get(attr::decoding).is_none() {
                element.attrs.push(attr::decoding, "async");
            }
        }
        for child in element.children.make_mut() {
            if let HtmlNode::Element(child) = child {
                Self::visit(child);
            }
        }
    }
}
