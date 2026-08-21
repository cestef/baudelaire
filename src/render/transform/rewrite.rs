//! Structure-aware link rewriting over typst's own HTML DOM.

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;
use crate::render::links::{Link, Target};
use crate::render::origin::Origins;
use crate::render::transform::{Cx, DocumentExt, ElementExt, Exempt, Transform};

/// The core [`Transform`]: resolves internal `.typ` source-path links to
/// permalinks, recording the broken ones, every map entry it consulted
/// (whether or not it matched, so a page rebuilds when the URL layout it
/// resolved against changes), and the pages this page points at.
///
/// Only links written in the content tree are collected for the graph, never a
/// template's nav or a listing's generated entries.
pub(super) struct Links;

impl Transform for Links {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let (config, page, links) = (cx.config, cx.page, cx.links);
        let origins = Origins::new(cx.world);
        let lang = config.multilingual().then_some(page.lang.as_str());
        let content = (page.authored() && config.graph()).then_some(cx.content);
        let found = &mut cx.found;
        doc.walk(|element| {
            let span = element.span;
            let anchor = element.tag == tag::a;
            let authored = || {
                content
                    .as_ref()
                    .is_some_and(|dir| origins.authored(span, dir, page))
            };
            element.rewrite(&[attr::href, attr::src], |value| {
                if anchor && let Some(target) = Self::fragment(value, &page.permalink) {
                    found.deep.push(target);
                    return None;
                }
                let resolution = links.classify(value, &page.source, lang);
                found.links.extend(resolution.probed);
                match resolution.link {
                    Link::Resolved(target) => {
                        let url = target.to_string();
                        if target.fragment().is_some() {
                            found.deep.push(target.clone());
                        }
                        if authored() {
                            found.outbound.record(target, &page.permalink);
                        }
                        Some(url)
                    }
                    Link::Broken => {
                        found.broken.push(value.to_owned());
                        None
                    }
                    Link::Passthrough => {
                        if authored() {
                            let serving = links.served(value);
                            found.urls.extend(serving.probed);
                            if let Some(target) = serving.target {
                                found.outbound.record(target, &page.permalink);
                            }
                        }
                        None
                    }
                }
            });
        });
    }
}

impl Links {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "links";

    /// The deep link a bare `#fragment` makes: it points into the page it was
    /// written on, so the target is that page's own permalink.
    ///
    /// `None` for anything that is not a fragment, and for a bare `#`, which
    /// names no section.
    fn fragment(raw: &str, permalink: &str) -> Option<Target> {
        if !raw.starts_with('#') {
            return None;
        }
        let target = Target::from(format!("{permalink}{raw}"));
        if target.fragment().is_some() {
            Some(target)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Links;

    /// A fragment written on `/posts/a/` names a section of `/posts/a/`, so the
    /// site-wide check has a page to look the heading up on.
    #[test]
    fn a_bare_fragment_targets_the_page_it_was_written_on() {
        let target = Links::fragment("#install", "/posts/a/").expect("a fragment");

        assert_eq!(target.page(), "/posts/a/");
        assert_eq!(target.fragment(), Some("install"));
        assert_eq!(target.to_string(), "/posts/a/#install");
    }

    /// A `?` after the `#` is part of the fragment, since that is what a
    /// browser resolves; anything that is not a fragment, and a `#` that names
    /// no section, are not deep links.
    #[test]
    fn only_a_fragment_naming_a_section_is_a_deep_link() {
        let queried = Links::fragment("#install?x=1", "/a/").expect("a fragment");
        assert_eq!(queried.fragment(), Some("install?x=1"));

        for raw in ["#", "b.typ", "b.typ#install", "/a/#install", ""] {
            assert!(Links::fragment(raw, "/a/").is_none(), "{raw}");
        }
    }
}
