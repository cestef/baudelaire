//! Pieces of a page's markup, re-serialized out of the typed DOM.
//!
//! Two consumers, with the same two needs: markup without the document wrapper
//! typst-html always writes, and markup that survives for a cache-served page,
//! long after the DOM is gone. [`Fragments`] is what the single-file export
//! assembles; [`Syndicated`] is the prose a full-content feed publishes.
//!
//! Captured here, at compile time, rather than sliced out of the finished page,
//! so nothing ever takes a rendered page apart as text.

use serde::{Deserialize, Serialize};
use typst::diag::SourceResult;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode, HtmlOptions, HtmlTag, attr, tag};

use crate::config::{BaseUrl, RegionConfig};

use super::transform::ElementExt;

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
}

/// Loose DOM nodes, re-serialized by typst-html's own pass.
///
/// typst-html serializes a *document*, so a piece of one is handed to it as the
/// root of a bare `<template>` and that wrapper peeled back off. Both ends of
/// the envelope are constants this module asked for one call earlier, so it is
/// unwrapping its own output rather than parsing markup.
///
/// One owner, because both captures in this module need it and the peeling is
/// the part that would quietly cut into real markup if the two drifted.
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
    /// serializer's output. Every affix is a constant we just asked for, so an
    /// unexpected shape leaves the text alone rather than cutting into it.
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
///
/// Captured from the DOM rather than sliced out of the finished page, and that
/// is the whole point. A feed entry travels to a reader that has no page to
/// resolve `/posts/b/` or `/assets/app.abc.css` against, so the URLs have to be
/// rewritten, and rewriting serialized markup as a string is what the typed DOM
/// exists to avoid. It is the same argument the social card's `og:image` is
/// absolutized under.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Syndicated(pub String);

impl Syndicated {
    /// Capture the prose a feed carries for this page.
    ///
    /// The region, or `<body>` when the layout emits no such element. Never the
    /// whole document: it begins `<!DOCTYPE html><head>`, and a layout that
    /// names no region would otherwise publish its every `<meta>` tag as the
    /// entry's body. The search index makes the opposite choice, because it
    /// would rather index a page whole than not at all.
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
            element.assets(|url| Some(BaseUrl::resolve(base, url)));
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
    ///
    /// The same three rules the search index applies to the finished text
    /// (`Text::skipped`), stated here against the typed DOM because the two see
    /// different things: a raw element a reader never reads, one the site named
    /// in `region { ignore }`, and one that declares itself hidden from
    /// assistive technology. That last covers a heading's own self link, which
    /// says nothing the heading has not and would travel as a stray `#`.
    fn chrome(element: &HtmlElement, ignore: &[String]) -> bool {
        element.tag == tag::script
            || element.tag == tag::style
            || element
                .attrs
                .get(attr::aria_hidden)
                .is_some_and(|v| v == "true")
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
    ///
    /// Anywhere, not just under the root: a layout is free to put its `<main>`
    /// inside a wrapper, and typst-html's own `<body>` is a child of the root.
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

    /// Anything that is not the envelope we wrote is returned untouched, so a
    /// changed serializer degrades to "too much markup" rather than to markup
    /// with its first tag sliced off.
    #[test]
    fn unwrap_leaves_unrecognized_output_alone() {
        assert_eq!(Markup::unwrap("<p>hi</p>"), "<p>hi</p>");
        assert_eq!(
            Markup::unwrap("<!DOCTYPE html><html><body>hi</body></html>"),
            "<html><body>hi</body></html>"
        );
    }
}
