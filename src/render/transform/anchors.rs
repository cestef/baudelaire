//! Gives every heading a slug `id`, so sections are deep-linkable.
//!
//! When `html { anchors true }` is set (the default), each `<h1>`..`<h6>` without
//! an explicit `id` gets one derived from its text (the same slug rule as page
//! URLs). An author-set `id` is always left untouched. A heading whose text has
//! no URL-safe characters is skipped rather than given an empty anchor.

use std::collections::BTreeSet;

use typst_html::{HtmlDocument, attr};

use crate::config::Config;
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
        if cx.config.html.anchors {
            doc.walk(|element| {
                if element.heading().is_none() || element.attrs.get(attr::id).is_some() {
                    return;
                }
                if let Some(slug) = Slug::parse(&element.text()) {
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
