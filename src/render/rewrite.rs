//! Structure-aware link rewriting over typst's own HTML DOM.

use std::path::Path;

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr};

use crate::config::Config;
use crate::render::links::Link;
use crate::render::LinkMap;
use crate::render::transform::{Cx, Transform};

/// The core [`Transform`]: resolves internal `.typ` source-path links to
/// permalinks. Always runs — it is URL resolution, not an optional pass — and
/// records broken internal links in the context for link checking.
pub(super) struct Links;

impl Transform for Links {
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let broken = LinkRewriter::new(cx.links, &cx.page.source).run(doc);
        cx.broken.extend(broken);
    }
}

/// Walks a rendered document's element tree, rewriting internal `href`/`src`
/// attributes to resolved permalinks and recording any that point at a
/// non-existent `.typ` page. Operates on the typed DOM — never on the
/// serialized string — so it can't corrupt markup.
pub(super) struct LinkRewriter<'a> {
    links: &'a LinkMap,
    from: &'a Path,
    /// Raw targets of internal `.typ` links with no matching page.
    broken: Vec<String>,
}

impl<'a> LinkRewriter<'a> {
    pub(super) fn new(links: &'a LinkMap, from: &'a Path) -> Self {
        Self {
            links,
            from,
            broken: Vec::new(),
        }
    }

    /// Rewrite `doc` in place, returning the raw targets of any broken links.
    pub(super) fn run(mut self, doc: &mut HtmlDocument) -> Vec<String> {
        self.visit(doc.root_mut());
        self.broken
    }

    fn visit(&mut self, element: &mut HtmlElement) {
        for key in [attr::href, attr::src] {
            let Some(value) = element.attrs.get_mut(key) else {
                continue;
            };
            match self.links.classify(value, self.from) {
                Link::Resolved(url) => *value = url.into(),
                Link::Broken => self.broken.push(value.to_string()),
                Link::Passthrough => {}
            }
        }
        for child in element.children.make_mut() {
            if let HtmlNode::Element(child) = child {
                self.visit(child);
            }
        }
    }
}
