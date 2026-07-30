//! Gives every heading a slug `id`, so sections are deep-linkable.
//!
//! When `html { anchors true }` is set (the default), each `<h1>`..`<h6>` without
//! an explicit `id` gets one derived from its text (the same slug rule as page
//! URLs). An author-set `id` is always left untouched. A heading whose text has
//! no URL-safe characters is skipped rather than given an empty anchor.

use std::collections::HashSet;

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, HtmlTag, attr, tag};

use crate::config::Config;
use crate::content::Slug;

use super::{Cx, DocumentExt, Transform};

/// The elements a reader deep-links to, spelled once.
const HEADINGS: &[HtmlTag] = &[tag::h1, tag::h2, tag::h3, tag::h4, tag::h5, tag::h6];

/// The [`Transform`] that adds heading `id` anchors.
pub(super) struct Anchors;

impl Transform for Anchors {
    /// Always. `html { anchors }` decides whether ids are *derived*, not whether
    /// this pass runs: it is also the only pass that knows a page's full id set,
    /// which the deep-link check reads. Gating the whole thing left a site with
    /// `anchors #false` reporting every fragment link as broken, having simply
    /// never looked.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        // ids are unique per page: like-slugged headings get `-2`, `-3`, .. suffixes.
        // Every authored id is collected up front (on any element, anywhere in
        // the document) so a derived id never collides with one, regardless of
        // which comes first in the walk.
        let mut seen: HashSet<String> = HashSet::new();
        doc.walk(|element| {
            if let Some(id) = element.attrs.get(attr::id) {
                seen.insert(id.to_string());
            }
        });
        if cx.config.html.anchors {
            doc.walk(|element| {
                if !Self::heading(element.tag) || element.attrs.get(attr::id).is_some() {
                    return;
                }
                if let Some(slug) = Slug::parse(&Self::text(element)) {
                    let id = Self::unique(slug.into_string(), &mut seen);
                    element.attrs.push(attr::id, id.as_str());
                }
            });
        }
        // Every id on the page, authored or derived, is what a deep link from
        // elsewhere may name. Recorded here because this is the pass that knows
        // the full set, and the check that needs it is site-wide.
        cx.found.anchors.extend(seen);
    }
}

impl Anchors {
    /// Whether `tag` is one of `<h1>`..`<h6>`.
    fn heading(tag: HtmlTag) -> bool {
        HEADINGS.contains(&tag)
    }

    /// `base`, or the first `base-N` (N≥2) not already taken, reserving the
    /// result in `seen`.
    fn unique(base: String, seen: &mut HashSet<String>) -> String {
        if seen.insert(base.clone()) {
            return base;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}-{n}");
            if seen.insert(candidate.clone()) {
                return candidate;
            }
            n += 1;
        }
    }

    /// An element's visible text, concatenating descendant text nodes, enough to
    /// slug a heading whose content is styled inline (`<code>`, `<em>`, ..).
    fn text(element: &HtmlElement) -> String {
        let mut out = String::new();
        Self::gather(element, &mut out);
        out
    }

    fn gather(element: &HtmlElement, out: &mut String) {
        for child in &element.children {
            match child {
                HtmlNode::Text(text, _) => out.push_str(text),
                HtmlNode::Element(el) => Self::gather(el, out),
                _ => {}
            }
        }
    }
}
