//! Moves the footnote list back inside the page's content.
//!
//! Typst appends a page's footnotes to the end of the document as a
//! `<section role="doc-endnotes">`. On a page with no template that is exactly
//! right: the section is the last thing in the body. On a templated page it is
//! not, because everything the layout emits, its header, its footer, its
//! scripts, is already in the body by then, so the notes land *after the site
//! footer*, outside `<main>` and outside whatever element the layout uses to
//! set the content width. A reader gets a full-bleed, unstyled list below the
//! footer, and a stylesheet cannot fix a placement in the DOM.
//!
//! `html { footnotes }` names the element they belong in instead: the page's
//! `article`, its `main`, or `end` to leave them where Typst put them. The
//! configured element is searched for depth-first, and `article` falls back to
//! `main`, so a layout that uses only one of the two needs no configuration.
//! Nothing moves when neither is present: a page whose layout emits no
//! container has nowhere better to put them, and inventing one would be a
//! wrapper the author never wrote.

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, HtmlTag, attr, tag};

use crate::config::{Config, Footnotes as Placement};

use super::{Cx, Transform};

/// The `role` Typst marks the footnote list with, and the only thing this pass
/// matches on: it is the DPUB ARIA name for the list, so it holds whatever the
/// element is called.
const ENDNOTES: &str = "doc-endnotes";

/// The [`Transform`] that relocates the footnote list.
pub(super) struct Footnotes;

impl Transform for Footnotes {
    /// `end` is Typst's own placement, so there is nothing to do.
    fn enabled(&self, config: &Config) -> bool {
        config.html.footnotes != Placement::End
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let targets = Self::targets(cx.config.html.footnotes);
        let Some(body) = Self::body(doc.root_mut()) else {
            return;
        };
        let Some(notes) = Self::take(body) else {
            return;
        };
        // Put it back exactly where it was if no container wants it: a page that
        // loses its footnotes is worse than one that renders them low.
        if let Some(orphan) = Self::place(body, targets, notes) {
            body.children.push(orphan);
        }
    }
}

impl Footnotes {
    /// The elements to try, in order. `article` falls back to `main` because a
    /// layout that wraps its content in only one of the two is the common case,
    /// and leaving the notes stranded over a missing `article` would be a
    /// configuration error the author never made.
    fn targets(placement: Placement) -> &'static [HtmlTag] {
        match placement {
            Placement::Article => &[tag::article, tag::main],
            Placement::Main => &[tag::main],
            // Unreachable through `enabled`, and cheaper to answer than to
            // assert: an empty list moves nothing.
            Placement::End => &[],
        }
    }

    /// The document's `<body>`, which is where Typst appends the section and the
    /// only subtree this pass touches.
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
    /// Direct children only: Typst appends it there, and a `role="doc-endnotes"`
    /// deeper in the tree is the author's own markup, which is not this pass's
    /// to move.
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
    ///
    /// Not [`ElementExt::walk`]: that hands each element to a closure and cannot
    /// hand one back out, and this pass needs the element itself to append to.
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
