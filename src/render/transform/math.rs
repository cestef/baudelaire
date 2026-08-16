//! Takes typst's inline equation styles out of a page's `<head>`, and says the
//! page wants the served [`MathSheet`] instead.

use typst_html::{HtmlDocument, HtmlNode, tag};

use crate::config::{Config, MathStyles};
use crate::owned::{MathSheet, Owned};

use super::{Cx, DocumentExt, ElementExt, Transform};

/// The [`Transform`] that takes typst's inline equation styles out and asks for
/// the served stylesheet in their place.
pub(super) struct Math;

impl Transform for Math {
    /// Whenever the styles are not being left where typst put them.
    fn enabled(&self, config: &Config) -> bool {
        MathStyles::of(&config.html).hoisted()
    }

    /// Removing nothing means the page had no equation, and so must not be
    /// given a stylesheet it has no use for.
    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let Some(head) = doc.head() else {
            return;
        };
        let before = head.children.len();
        head.children.retain(|node| !Self::injected(node));
        if head.children.len() == before {
            return;
        }
        if MathStyles::of(&cx.config.html).served() {
            cx.found.owned.insert(
                Owned::rel(&MathSheet, cx.config)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

impl Math {
    /// Whether this node is the `<style>` typst injected: an element with that
    /// tag, no attributes of its own, and exactly the pinned text.
    fn injected(node: &HtmlNode) -> bool {
        let HtmlNode::Element(element) = node else {
            return false;
        };
        element.tag == tag::style
            && element.attrs.0.is_empty()
            && MathSheet::injected(&element.text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Span;
    use typst_html::{HtmlElement, attr};

    fn style(text: &str) -> HtmlNode {
        let mut element = HtmlElement::new(tag::style);
        element
            .children
            .push(HtmlNode::Text(text.into(), Span::detached()));
        element.into()
    }

    #[test]
    fn recognises_the_injected_block_across_the_trailing_newline() {
        assert!(Math::injected(&style(MathSheet::text())));
        assert!(Math::injected(&style(MathSheet::text().trim_end())));
    }

    #[test]
    fn leaves_a_style_that_is_not_the_injected_one() {
        assert!(!Math::injected(&style("mfrac { padding-inline: 0; }")));
        assert!(!Math::injected(&style("")));
    }

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
