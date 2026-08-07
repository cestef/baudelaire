//! Gives every heading a slug `id`, so sections are deep-linkable, and
//! optionally the link a reader clicks to copy it.
//!
//! With `html { anchors }` on (the default), each `<h1>`..`<h6>` without an
//! explicit `id` gets one derived from its text (the same slug rule as page
//! URLs). An author-set `id` is always left untouched. A heading whose text has
//! no URL-safe characters is skipped rather than given an empty anchor.
//!
//! `anchors { link "#" }` adds an `<a class="anchor" href="#id">` inside the
//! heading. Off by default: it is markup the site did not write, and a theme
//! with no rule for it gets a stray `#` in its titles.

use std::collections::BTreeSet;

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{AnchorConfig, Config, Place};
use crate::content::Slug;

use super::{Cx, DocumentExt, ElementExt, Transform};

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
        //
        // Ordered, and not merely a set, because it leaves this pass: it is
        // drained into `found.anchors`, which is serialized into the build
        // manifest, so a hashed one wrote a different manifest on every build
        // of an unchanged site.
        let mut seen: BTreeSet<String> = BTreeSet::new();
        doc.walk(|element| {
            if let Some(id) = element.attrs.get(attr::id) {
                seen.insert(id.to_string());
            }
        });
        let anchors = &cx.config.html.anchors;
        if anchors.enabled {
            doc.walk(|element| {
                // A heading the site did not ask to cover keeps whatever it has:
                // no derived id, and so nothing to link to either.
                if !element.heading().is_some_and(|level| anchors.covers(level)) {
                    return;
                }
                // An authored id is the author's, and is still worth linking to.
                let id = match element.attrs.get(attr::id) {
                    Some(id) => id.to_string(),
                    None => match Slug::parse(&element.text()) {
                        Some(slug) => {
                            let id = Self::unique(slug.into_string(), &mut seen);
                            element.attrs.push(attr::id, id.as_str());
                            id
                        }
                        // Nothing URL-safe survives, so there is no id to link
                        // to and an empty one would link to the page itself.
                        None => return,
                    },
                };
                if let Some(text) = &anchors.link {
                    Self::link(element, &id, text, anchors.place);
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
    /// Put the self link inside `heading`, on the side `place` names.
    ///
    /// Inside rather than beside, because a heading is what a reader is pointing
    /// at: a sibling would sit outside the element a table of contents and a
    /// stylesheet both reach for, and would break the `h2 + p` selectors a theme
    /// is written with.
    ///
    /// `aria-hidden` and a negative `tabindex`: the link says nothing a screen
    /// reader has not just read out as the heading, and a keyboard user tabbing
    /// through a long page should not stop at one per section.
    fn link(heading: &mut HtmlElement, id: &str, text: &str, place: Place) {
        let link = HtmlNode::from(
            HtmlElement::new(tag::a)
                .with_attr(attr::href, format!("#{id}"))
                .with_attr(attr::class, AnchorConfig::CLASS)
                .with_attr(attr::aria_hidden, "true")
                .with_attr(attr::tabindex, "-1")
                // Detached: the link is this crate's markup, not the author's,
                // so there is no source position to stamp it with.
                .with_children(
                    std::iter::once(HtmlNode::Text(text.into(), typst::syntax::Span::detached()))
                        .collect(),
                ),
        );
        match place {
            Place::After => heading.children.push(link),
            Place::Before => heading.children.insert(0, link),
        }
    }

    /// `base`, or the first `base-N` (N≥2) not already taken, reserving the
    /// result in `seen`.
    fn unique(base: String, seen: &mut BTreeSet<String>) -> String {
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
}

#[cfg(test)]
mod tests {
    use super::Anchors;
    use std::collections::BTreeSet;

    /// Like-slugged headings get `-2`, `-3`, .. and each is reserved as it is
    /// handed out, so no later heading can be given an id an earlier one took.
    ///
    /// The order the set comes back in is the assertion that matters: it is
    /// drained into the page's recorded anchors, which go into the build
    /// manifest, and a hashed set wrote a different list on every build of an
    /// unchanged page.
    #[test]
    fn ids_are_reserved_as_they_are_handed_out_and_come_back_ordered() {
        let mut seen = BTreeSet::new();

        assert_eq!(Anchors::unique("usage".to_owned(), &mut seen), "usage");
        assert_eq!(Anchors::unique("usage".to_owned(), &mut seen), "usage-2");
        assert_eq!(Anchors::unique("usage".to_owned(), &mut seen), "usage-3");
        assert_eq!(Anchors::unique("api".to_owned(), &mut seen), "api");

        assert_eq!(
            seen.into_iter().collect::<Vec<_>>(),
            ["api", "usage", "usage-2", "usage-3"]
        );
    }
}
