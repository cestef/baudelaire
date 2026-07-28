//! A page's markup, captured in the pieces the single-file export assembles.
//!
//! The export needs each page's contents without the document wrapper
//! typst-html always writes, and it needs them for cache-served pages too, long
//! after the DOM is gone. Both are satisfied by re-serializing the typed DOM at
//! compile time, so nothing ever takes a rendered page apart as text.

use serde::{Deserialize, Serialize};
use typst::diag::SourceResult;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode, HtmlOptions, HtmlTag, attr, tag};

/// The doctype [`typst_html::html`] writes ahead of any root element, and the
/// wrapper this module serializes through. Both are strings *we* produced one
/// call earlier, so stripping them is unwrapping our own envelope, not parsing
/// markup.
const DOCTYPE: &str = "<!DOCTYPE html>";
const OPEN: &str = "<template>";
const CLOSE: &str = "</template>";

/// One page's contents, split into the pieces a shared document is built from.
///
/// Every field defaults, so a manifest written by an older layout still parses
/// and the cache's own schema decides whether to trust it (see
/// [`crate::graph::Renderer`]), rather than warning every user once per upgrade.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Fragments {
    /// The `<head>` contents: the charset, the meta tags, the title.
    pub head: String,
    /// The page's external resource elements (`<link>`, and `<script src>`),
    /// wherever they sat, one serialized element per entry.
    ///
    /// Lifted out because a typst template cannot emit `<head>` and so writes
    /// its stylesheet link and its bundle into the body. Left where they were,
    /// every route of the export would carry its own copy: with `html { embed }`
    /// on, that is the whole stylesheet and the whole bundle, inlined per page.
    pub resources: Vec<String>,
    /// The `<body>` contents, with those elements taken out.
    pub body: String,
}

impl Fragments {
    /// Capture a compiled page's markup. Serialization is the same pass
    /// typst-html runs for the page itself, over a cheap clone of the document,
    /// so links and frames resolve exactly as they do in the real output.
    pub fn capture(doc: &HtmlDocument, options: &HtmlOptions) -> SourceResult<Self> {
        let mut doc = doc.clone();
        let mut lifted = Vec::new();
        Self::lift(doc.root_mut(), &mut lifted);
        Ok(Self {
            head: Self::markup(&doc, Self::children(&doc, tag::head), options)?,
            resources: lifted
                .into_iter()
                .map(|el| Self::markup(&doc, [HtmlNode::Element(el)], options))
                .collect::<SourceResult<_>>()?,
            body: Self::markup(&doc, Self::children(&doc, tag::body), options)?,
        })
    }

    /// Whether an element only references a resource, and so says the same
    /// thing wherever in the document it sits.
    fn resource(el: &HtmlElement) -> bool {
        el.tag == tag::link || (el.tag == tag::script && el.attrs.get(attr::src).is_some())
    }

    /// Take every resource element out of the tree, depth-first, leaving the
    /// rest in place.
    ///
    /// A lifted script is marked `defer`: it may end up in the exported file's
    /// `<head>`, where a classic script would otherwise run before the page it
    /// expects to find. `defer` puts it back after parsing, which is where it
    /// ran when it sat in the body.
    fn lift(element: &mut HtmlElement, out: &mut Vec<HtmlElement>) {
        let mut kept = Vec::with_capacity(element.children.len());
        for node in &element.children {
            match node {
                HtmlNode::Element(el) if Self::resource(el) => {
                    let mut el = el.clone();
                    if el.tag == tag::script && el.attrs.get(attr::defer).is_none() {
                        el.attrs.push(attr::defer, "");
                    }
                    out.push(el);
                }
                other => kept.push(other.clone()),
            }
        }
        element.children = kept.into_iter().collect();
        for node in element.children.make_mut() {
            if let HtmlNode::Element(child) = node {
                Self::lift(child, out);
            }
        }
    }

    /// The root's `which` child's children, empty when the page has no such
    /// child (a template that emitted its own root, say).
    fn children(doc: &HtmlDocument, which: HtmlTag) -> Vec<HtmlNode> {
        doc.root()
            .children
            .iter()
            .find_map(|node| match node {
                HtmlNode::Element(el) if el.tag == which => Some(el.children.to_vec()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Serialize loose nodes by handing them to typst-html as the root of a
    /// bare wrapper, then peeling that wrapper back off.
    fn markup(
        doc: &HtmlDocument,
        nodes: impl IntoIterator<Item = HtmlNode>,
        options: &HtmlOptions,
    ) -> SourceResult<String> {
        let mut doc = doc.clone();
        *doc.root_mut() =
            HtmlElement::new(tag::template).with_children(nodes.into_iter().collect());
        let html = typst_html::html(&doc, options)?;
        Ok(Self::unwrap(&html).to_owned())
    }

    /// Peel the doctype and the attribute-less wrapper element back off the
    /// serializer's output. Every affix is a constant we just asked for, so an
    /// unexpected shape leaves the text alone rather than cutting into it.
    fn unwrap(html: &str) -> &str {
        let html = html.trim();
        let html = html.strip_prefix(DOCTYPE).unwrap_or(html).trim();
        let html = html.strip_prefix(OPEN).unwrap_or(html);
        html.strip_suffix(CLOSE).unwrap_or(html).trim()
    }
}

#[cfg(test)]
mod tests {
    use super::Fragments;

    #[test]
    fn unwrap_peels_the_doctype_and_wrapper() {
        assert_eq!(
            Fragments::unwrap("<!DOCTYPE html><template><p>hi</p></template>"),
            "<p>hi</p>"
        );
        assert_eq!(
            Fragments::unwrap("<!DOCTYPE html>\n<template>\n  <p>hi</p>\n</template>\n"),
            "<p>hi</p>"
        );
    }

    /// Anything that is not the envelope we wrote is returned untouched, so a
    /// changed serializer degrades to "too much markup" rather than to markup
    /// with its first tag sliced off.
    #[test]
    fn unwrap_leaves_unrecognized_output_alone() {
        assert_eq!(Fragments::unwrap("<p>hi</p>"), "<p>hi</p>");
        assert_eq!(
            Fragments::unwrap("<!DOCTYPE html><html><body>hi</body></html>"),
            "<html><body>hi</body></html>"
        );
    }
}
