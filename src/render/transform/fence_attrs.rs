//! Moves the attributes `/.baudelaire/site/html.typ`'s `fence()` and a markdown
//! fence's parameters put on a wrapper onto the code block's own `<pre>`.

use std::sync::LazyLock;

use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, tag};

use crate::config::Config;
use crate::world::module::Html;

use super::{AttrsExt, Cx, DocumentExt, Transform};

static MARKER: LazyLock<HtmlAttr> =
    LazyLock::new(|| HtmlAttr::intern(Html::FENCE).expect("marker is a valid attribute name"));

/// The [`Transform`] that lifts a fence's attributes onto its block.
pub(super) struct FenceAttrs;

impl Transform for FenceAttrs {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[]
    }

    /// Every build: the wrapper is emitted whenever a fence carries a
    /// parameter, and a page that kept it would ship a `div` nobody wrote.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, _cx: &mut Cx<'_>) {
        doc.walk(Self::unwrap);
    }
}

impl FenceAttrs {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "fence-attrs";

    /// Replace each wrapper among `element`'s children by its children, the
    /// wrapper's attributes moved onto every `<pre>` among them.
    ///
    /// Repeated until none is left, since a splice can lift a nested wrapper
    /// past where the walk has already descended.
    fn unwrap(element: &mut HtmlElement) {
        while element.children.iter().any(Self::wrapper) {
            let mut kept = typst::ecow::EcoVec::new();
            for node in &element.children {
                match node {
                    HtmlNode::Element(wrapper) if Self::wrapper(node) => {
                        kept.extend(
                            wrapper
                                .children
                                .iter()
                                .map(|child| Self::lift(wrapper, child)),
                        );
                    }
                    other => kept.push(other.clone()),
                }
            }
            element.children = kept;
        }
    }

    /// `child`, carrying the wrapper's attributes when it is a `<pre>`.
    fn lift(wrapper: &HtmlElement, child: &HtmlNode) -> HtmlNode {
        let HtmlNode::Element(pre) = child else {
            return child.clone();
        };
        if pre.tag != tag::pre {
            return child.clone();
        }
        let mut pre = pre.clone();
        for (key, value) in wrapper.attrs.0.iter().filter(|(key, _)| *key != *MARKER) {
            pre.attrs.set(*key, value);
        }
        HtmlNode::Element(pre)
    }

    fn wrapper(node: &HtmlNode) -> bool {
        matches!(node, HtmlNode::Element(element) if element.attrs.get(*MARKER).is_some())
    }
}
