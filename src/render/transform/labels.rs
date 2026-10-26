//! Gives every labelled element its label as an `id`, the way Typst already
//! does for one a link in the same document points at.

use typst::introspection::Introspector;
use typst_html::{HtmlDocument, create_link_anchors};

use crate::config::Config;

use super::{Cx, Exempt, Transform};

/// The [`Transform`] that makes every Typst label an anchor.
pub(super) struct Labels;

impl Transform for Labels {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

    fn enabled(&self, config: &Config) -> bool {
        config.html.anchors.enabled && config.html.anchors.labels
    }

    /// Typst assigns an `id` only to the elements something links to, and its own
    /// placement (an empty `<span>` for an element that rendered nothing, a wrapped
    /// text node, an anchor inside an inline SVG) is reused by naming every
    /// labelled location a target; one that already has an id keeps it.
    fn apply(&self, doc: &mut HtmlDocument, _cx: &mut Cx<'_>) {
        let targets = doc
            .introspector()
            .query_labelled()
            .iter()
            .filter_map(typst::foundations::Content::location)
            .collect();
        create_link_anchors(doc, &targets);
    }
}

impl Labels {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "labels";
}
