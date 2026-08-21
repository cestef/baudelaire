//! Gives every heading a slug `id`, so sections are deep-linkable, and
//! optionally the `anchors { link }` a reader clicks to copy it.

use std::collections::BTreeSet;

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{AnchorConfig, Config, Place};
use crate::content::Slug;

use super::{Cx, DocumentExt, ElementExt, Exempt, Transform};

/// The [`Transform`] that adds heading `id` anchors.
pub(super) struct Anchors;

impl Transform for Anchors {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

    /// Always: `html { anchors }` decides whether ids are *derived*, not
    /// whether this pass runs, and the deep-link check needs the id set it
    /// records either way.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    /// The id set is ordered, not hashed, because it is drained into the build
    /// manifest.
    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        doc.walk(|element| {
            if let Some(id) = element.attrs.get(attr::id) {
                seen.insert(id.to_string());
            }
        });
        let anchors = &cx.config.html.anchors;
        if anchors.enabled {
            doc.walk(|element| {
                if !element.heading().is_some_and(|level| anchors.covers(level)) {
                    return;
                }
                let id = match element.attrs.get(attr::id) {
                    Some(id) => id.to_string(),
                    None => match Slug::parse(&element.text()) {
                        Some(slug) => {
                            let id = Self::unique(slug.into_string(), &mut seen);
                            element.attrs.push(attr::id, id.as_str());
                            id
                        }
                        None => return,
                    },
                };
                if let Some(text) = &anchors.link {
                    Self::link(element, &id, text, anchors.place);
                }
            });
        }
        cx.found.anchors.extend(seen);
    }
}

impl Anchors {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "anchors";

    /// Put the self link inside `heading`, on the side `place` names.
    fn link(heading: &mut HtmlElement, id: &str, text: &str, place: Place) {
        let link = HtmlNode::from(
            HtmlElement::new(tag::a)
                .with_attr(attr::href, format!("#{id}"))
                .with_attr(attr::class, AnchorConfig::CLASS)
                .with_attr(attr::aria_hidden, "true")
                .with_attr(attr::tabindex, "-1")
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
