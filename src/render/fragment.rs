//! Pieces of a page's markup, re-serialized out of the typed DOM: [`Fragments`]
//! is what the single-file export assembles, [`Syndicated`] the prose a
//! full-content feed publishes.

use serde::{Deserialize, Serialize};
use typst::diag::SourceResult;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode, HtmlOptions, HtmlTag, attr, tag};

use crate::config::{BaseUrl, RegionConfig};

use super::transform::ElementExt;

/// The doctype [`typst_html::html`] writes ahead of any root element, and the
/// wrapper this module serializes through.
const DOCTYPE: &str = "<!DOCTYPE html>";
const OPEN: &str = "<template>";
const CLOSE: &str = "</template>";

/// One page's contents, split into the pieces a shared document is built from.
///
/// Every field defaults, so a manifest written by an older layout still parses
/// and the cache's own schema decides whether to trust it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Fragments {
    /// The `<head>` contents.
    pub head: String,
    /// The page's external resource elements (`<link>`, and `<script src>`),
    /// wherever they sat, one serialized element per entry.
    pub resources: Vec<String>,
    /// The `<body>` contents, with those elements taken out.
    pub body: String,
}

impl Fragments {
    /// Capture a compiled page's markup.
    pub fn capture(doc: &HtmlDocument, options: &HtmlOptions) -> SourceResult<Self> {
        let mut doc = doc.clone();
        let mut lifted = Vec::new();
        Self::lift(doc.root_mut(), &mut lifted);
        Ok(Self {
            head: Markup::of(&doc, Self::children(&doc, tag::head), options)?,
            resources: lifted
                .into_iter()
                .map(|el| Markup::of(&doc, [HtmlNode::Element(el)], options))
                .collect::<SourceResult<_>>()?,
            body: Markup::of(&doc, Self::children(&doc, tag::body), options)?,
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
    /// expects to find.
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
}

/// Loose DOM nodes, re-serialized by typst-html's own pass.
///
/// typst-html serializes a *document*, so a piece of one is handed to it as the
/// root of a bare `<template>` and that wrapper peeled back off.
struct Markup;

impl Markup {
    fn of(
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
    /// serializer's output, leaving an unexpected shape alone.
    fn unwrap(html: &str) -> &str {
        let html = html.trim();
        let html = html.strip_prefix(DOCTYPE).unwrap_or(html).trim();
        let html = html.strip_prefix(OPEN).unwrap_or(html);
        html.strip_suffix(CLOSE).unwrap_or(html).trim()
    }
}

/// One page's prose as a full-content feed publishes it: the markup of the
/// region `html { region }` names, with the site's chrome taken out and every
/// URL made absolute.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Syndicated(pub String);

impl Syndicated {
    /// Capture the prose a feed carries for this page: the region, or `<body>`
    /// when the layout emits no such element. URLs are rebased rather than
    /// resolved, the base path being already on every root-relative URL of a
    /// finished page.
    pub fn capture(
        doc: &HtmlDocument,
        options: &HtmlOptions,
        region: &RegionConfig,
        base: Option<&BaseUrl>,
    ) -> SourceResult<Self> {
        let mut doc = doc.clone();
        let root = doc.root_mut();
        Self::prune(root, &region.ignore);
        root.walk(&mut |element| {
            element.assets(|url| Some(BaseUrl::rebase(base, url)));
        });
        let found = HtmlTag::intern(&region.element)
            .ok()
            .and_then(|element| Self::find(doc.root(), element));
        let nodes = found
            .or_else(|| Self::find(doc.root(), tag::body))
            .unwrap_or_else(|| doc.root().children.to_vec());
        Markup::of(&doc, nodes, options).map(Self)
    }

    /// Whether an element is chrome rather than prose, and so goes with its
    /// contents.
    fn chrome(element: &HtmlElement, ignore: &[String]) -> bool {
        element.silent()
            || ignore
                .iter()
                .any(|name| element.tag.resolve().eq_ignore_ascii_case(name))
    }

    /// Drop every chrome element from the tree, depth-first.
    fn prune(element: &mut HtmlElement, ignore: &[String]) {
        element
            .children
            .retain(|node| !matches!(node, HtmlNode::Element(el) if Self::chrome(el, ignore)));
        for node in element.children.make_mut() {
            if let HtmlNode::Element(child) = node {
                Self::prune(child, ignore);
            }
        }
    }

    /// The children of the first `which` element anywhere in the tree.
    fn find(element: &HtmlElement, which: HtmlTag) -> Option<Vec<HtmlNode>> {
        if element.tag == which {
            return Some(element.children.to_vec());
        }
        element.children.iter().find_map(|node| match node {
            HtmlNode::Element(el) => Self::find(el, which),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Markup;

    #[test]
    fn unwrap_peels_the_doctype_and_wrapper() {
        assert_eq!(
            Markup::unwrap("<!DOCTYPE html><template><p>hi</p></template>"),
            "<p>hi</p>"
        );
        assert_eq!(
            Markup::unwrap("<!DOCTYPE html>\n<template>\n  <p>hi</p>\n</template>\n"),
            "<p>hi</p>"
        );
    }

    #[test]
    fn unwrap_leaves_unrecognized_output_alone() {
        assert_eq!(Markup::unwrap("<p>hi</p>"), "<p>hi</p>");
        assert_eq!(
            Markup::unwrap("<!DOCTYPE html><html><body>hi</body></html>"),
            "<html><body>hi</body></html>"
        );
    }
}
