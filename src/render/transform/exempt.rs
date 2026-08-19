//! Records what the author has kept the lint off, then takes the marker back
//! out of the page.
//!
//! The lint runs after every transform, so the marker cannot survive to be read
//! there: what survives is the set of spans it covered, which is what a finding
//! is matched against.

use typst::syntax::Span;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode};

use crate::config::Config;
use crate::render::lint::{EXEMPT, Exemption};

use super::{Cx, DocumentExt, Transform};

/// The [`Transform`] that reads the lint markers and removes them.
pub(super) struct Exempt;

impl Transform for Exempt {
    fn enabled(&self, config: &Config) -> bool {
        config.check.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut marked: Vec<(Exemption, Vec<Span>)> = Vec::new();
        doc.walk(|element| {
            if let Some(value) = element.attrs.get(EXEMPT) {
                let mut covered = Vec::new();
                Self::cover(element, &mut covered);
                marked.push((Exemption::parse(value), covered));
            }
        });
        for (exemption, covered) in marked {
            for span in covered {
                cx.exempt.insert(span, &exemption);
            }
        }
        doc.walk(Self::unwrap);
    }
}

impl Exempt {
    /// Every span under `element`, its own first: what a finding is tested
    /// against once the marker itself is gone. Text nodes carry spans too, and
    /// a code fence's lines are text, so the walk cannot stop at elements.
    fn cover(element: &HtmlElement, out: &mut Vec<Span>) {
        out.push(element.span);
        for node in &element.children {
            match node {
                HtmlNode::Text(_, span) => out.push(*span),
                HtmlNode::Element(child) => Self::cover(child, out),
                _ => {}
            }
        }
    }

    /// Splice a marker element's children into its parent, so the page is the
    /// one the author would have written without it.
    ///
    /// Repeated until none is left: a splice lifts a nested marker into this
    /// element, and the walk has already descended past where that child was.
    fn unwrap(element: &mut HtmlElement) {
        while element.children.iter().any(Self::marker) {
            let mut kept = typst::ecow::EcoVec::new();
            for node in &element.children {
                match node {
                    HtmlNode::Element(child) if child.attrs.get(EXEMPT).is_some() => {
                        kept.extend(child.children.iter().cloned());
                    }
                    other => kept.push(other.clone()),
                }
            }
            element.children = kept;
        }
    }

    fn marker(node: &HtmlNode) -> bool {
        matches!(node, HtmlNode::Element(child) if child.attrs.get(EXEMPT).is_some())
    }
}
