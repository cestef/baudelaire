//! Takes typst's inline equation styles out of a page's `<head>` and points it
//! at the served stylesheet instead.
//!
//! What the block is, why it can only be reached from here, and how it is
//! recognised are all on [`MathSheet`].

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{Config, MathStyles};
use crate::render::MathSheet;

use super::{Cx, DocumentExt, ElementExt, Transform};

/// The [`Transform`] that swaps typst's inline equation styles for a link to the
/// served stylesheet.
pub(super) struct Math;

impl Transform for Math {
    /// Whenever the styles are not being left where typst put them. There is
    /// nothing else to gate on: the block is only ever present on a page typst
    /// found an equation in, so a site with no math never reaches past the walk.
    fn enabled(&self, config: &Config) -> bool {
        MathStyles::of(&config.html).hoisted()
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        // Best-effort, like every other head pass: a page that emitted its own
        // root has no `<head>`, so there is nothing to lift and nothing to link.
        let Some(head) = doc.head() else {
            return;
        };
        let before = head.children.len();
        head.children.retain(|node| !Self::injected(node));
        // Nothing removed is a page with no equation on it, which must not be
        // given a stylesheet it has no use for.
        if head.children.len() == before {
            return;
        }
        if MathStyles::of(&cx.config.html).served() {
            head.children.push(Self::link(cx.config));
            // What tells the pipeline to actually write the file it named: it
            // reserved one before any page existed, and only a page can say
            // whether the site has an equation in it.
            cx.found.owned.insert(MathSheet::REL.to_owned());
        }
    }
}

impl Math {
    /// Whether this node is the `<style>` typst injected: an element with that
    /// tag, no attributes of its own, and exactly the pinned text.
    ///
    /// The attribute check is what keeps an author's `<style>` out of it. A
    /// template writing the same declarations by hand would still be lifted,
    /// which is the correct outcome anyway: the page ends up with the rules.
    fn injected(node: &HtmlNode) -> bool {
        let HtmlNode::Element(element) = node else {
            return false;
        };
        element.tag == tag::style
            && element.attrs.0.is_empty()
            && MathSheet::injected(&element.text())
    }

    /// The `<link>` to the served stylesheet, spelled as an authored reference
    /// would be: the fingerprint, embed and base-path passes run after this one
    /// and reach it the same way they reach a template's own.
    fn link(config: &Config) -> HtmlNode {
        HtmlElement::new(tag::link)
            .with_attr(attr::rel, "stylesheet")
            .with_attr(
                attr::href,
                config.asset_url(std::path::Path::new(MathSheet::REL)),
            )
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Span;

    fn style(text: &str) -> HtmlNode {
        let mut element = HtmlElement::new(tag::style);
        element
            .children
            .push(HtmlNode::Text(text.into(), Span::detached()));
        element.into()
    }

    /// The pinned text is what the pass keys on, whitespace at the edges aside:
    /// the file carries a trailing newline and the injected block does not.
    #[test]
    fn recognises_the_injected_block_across_the_trailing_newline() {
        assert!(Math::injected(&style(MathSheet::text())));
        assert!(Math::injected(&style(MathSheet::text().trim_end())));
    }

    /// A site's own `<style>` is left alone even when it says something similar:
    /// only the block typst wrote, in full, is ours to move.
    #[test]
    fn leaves_a_style_that_is_not_the_injected_one() {
        assert!(!Math::injected(&style("mfrac { padding-inline: 0; }")));
        assert!(!Math::injected(&style("")));
    }

    /// An attribute means somebody wrote it: typst's block carries none, and a
    /// `<style media>` or a `<style nonce>` is the author's to keep.
    #[test]
    fn leaves_the_same_text_when_it_carries_an_attribute() {
        let HtmlNode::Element(element) = style(MathSheet::text()) else {
            unreachable!("style builds an element")
        };
        assert!(!Math::injected(
            &element.with_attr(attr::media, "print").into()
        ));
    }
}
