//! The part of a rendered page that is its prose, as `html { region }` names
//! it, over the typed DOM.

use typst_html::{HtmlElement, HtmlNode, HtmlTag, tag};

use crate::config::RegionConfig;

use super::transform::ElementExt;

/// Which part of a page is prose: the region a layout puts it in, and the
/// chrome inside that region which is not prose.
///
/// The DOM counterpart of [`crate::engine::text::Region`], which reads the same
/// two settings off serialized HTML.
pub(crate) struct Prose<'a>(&'a RegionConfig);

impl<'a> From<&'a RegionConfig> for Prose<'a> {
    fn from(config: &'a RegionConfig) -> Self {
        Self(config)
    }
}

impl Prose<'_> {
    /// Whether an element is chrome rather than prose, and so goes with its
    /// contents.
    pub(crate) fn chrome(&self, element: &HtmlElement) -> bool {
        element.silent()
            || self
                .0
                .ignore
                .iter()
                .any(|name| element.tag.resolve().eq_ignore_ascii_case(name))
    }

    /// The element a page's prose lives in: the one named, the `<body>` when
    /// the layout emits no such element, or `root` itself when it emits
    /// neither, so a page without the region is read whole rather than not at
    /// all.
    pub(crate) fn region<'d>(&self, root: &'d HtmlElement) -> &'d HtmlElement {
        HtmlTag::intern(&self.0.element)
            .ok()
            .and_then(|which| Self::find(root, which))
            .or_else(|| Self::find(root, tag::body))
            .unwrap_or(root)
    }

    /// The first `which` element in `element`'s subtree, depth-first.
    fn find(element: &HtmlElement, which: HtmlTag) -> Option<&HtmlElement> {
        if element.tag == which {
            return Some(element);
        }
        element.children.iter().find_map(|node| match node {
            HtmlNode::Element(child) => Self::find(child, which),
            _ => None,
        })
    }

    /// Visit `root` and every prose element under it, depth-first, entering no
    /// chrome. `root` itself is never tested.
    pub(crate) fn visit(&self, root: &HtmlElement, f: &mut impl FnMut(&HtmlElement)) {
        f(root);
        for child in &root.children {
            if let HtmlNode::Element(child) = child
                && !self.chrome(child)
            {
                self.visit(child, f);
            }
        }
    }
}
