//! Moves the footnote list Typst appended to the body into the elements
//! `html { footnotes "article" "main" }` names, most specific first.

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, HtmlTag, attr, tag};

use crate::config::Config;

use super::{Cx, Exempt, Transform};

/// The `role` Typst marks the footnote list with, and the only thing this pass
/// matches on.
const ENDNOTES: &str = "doc-endnotes";

/// The [`Transform`] that relocates the footnote list.
pub(super) struct Footnotes;

impl Transform for Footnotes {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

    /// Naming no element is Typst's own placement, so there is nothing to do.
    fn enabled(&self, config: &Config) -> bool {
        !config.html.footnotes.disabled()
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let targets: Vec<HtmlTag> = cx
            .config
            .html
            .footnotes
            .targets()
            .iter()
            .filter_map(|name| HtmlTag::intern(name).ok())
            .collect();
        let Some(body) = Self::body(doc.root_mut()) else {
            return;
        };
        let Some(notes) = Self::take(body) else {
            return;
        };
        if let Some(orphan) = Self::place(body, &targets, notes) {
            body.children.push(orphan);
        }
    }
}

impl Footnotes {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "footnotes";

    /// The document's `<body>`, the only subtree this pass touches.
    fn body(root: &mut HtmlElement) -> Option<&mut HtmlElement> {
        root.children
            .make_mut()
            .iter_mut()
            .find_map(|node| match node {
                HtmlNode::Element(el) if el.tag == tag::body => Some(el),
                _ => None,
            })
    }

    /// Remove the footnote list from `body`'s own children and return it.
    ///
    /// Direct children only, since a `role="doc-endnotes"` deeper in the tree
    /// is the author's own markup.
    fn take(body: &mut HtmlElement) -> Option<HtmlNode> {
        let index = body.children.iter().position(|node| match node {
            HtmlNode::Element(el) => Self::endnotes(el),
            _ => false,
        })?;
        let mut kept: Vec<HtmlNode> = body.children.iter().cloned().collect();
        let notes = kept.remove(index);
        body.children = kept.into_iter().collect();
        Some(notes)
    }

    /// Whether an element is the footnote list.
    fn endnotes(el: &HtmlElement) -> bool {
        el.tag == tag::section && el.attrs.get(attr::role).is_some_and(|r| r == ENDNOTES)
    }

    /// Append `notes` to the first element matching any of `targets`,
    /// depth-first. Returns the node back when nothing matched.
    fn place(body: &mut HtmlElement, targets: &[HtmlTag], notes: HtmlNode) -> Option<HtmlNode> {
        for target in targets {
            if let Some(element) = Self::find(body, *target) {
                element.children.push(notes);
                return None;
            }
        }
        Some(notes)
    }

    /// The first element with `tag` in `element`'s subtree, depth-first.
    fn find(element: &mut HtmlElement, tag: HtmlTag) -> Option<&mut HtmlElement> {
        if element.tag == tag {
            return Some(element);
        }
        element
            .children
            .make_mut()
            .iter_mut()
            .filter_map(|node| match node {
                HtmlNode::Element(child) => Some(child),
                _ => None,
            })
            .find_map(|child| Self::find(child, tag))
    }
}
