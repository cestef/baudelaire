//! Structure-aware link rewriting over typst's own HTML DOM.

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;
use crate::render::links::{Link, Target};
use crate::render::origin::Origins;
use crate::render::transform::{Cx, DocumentExt, ElementExt, Transform};

/// The core [`Transform`]: resolves internal `.typ` source-path links to
/// permalinks. Always runs (it is URL resolution, not an optional pass) and
/// records, in the context, the broken internal links (for link checking), every
/// map entry it consulted (for the build cache), and the pages this page's own
/// content points at (for backlinks). Operates on the typed DOM (never on the
/// serialized string) so it can't corrupt markup.
pub(super) struct Links;

impl Transform for Links {
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        // Read out of the context before the accumulator is borrowed: each is a
        // shared reference the context merely carries, so copying it out costs
        // nothing and leaves `found` the only thing borrowed from `cx`.
        let (config, page, links) = (cx.config, cx.page, cx.links);
        // Where an element was written, which is what tells this page's own
        // links from its layout's.
        let origins = Origins::new(cx.world);
        let lang = config.multilingual().then_some(page.lang.as_str());
        // Which of this page's links are *its own*, rather than its layout's.
        // A generated listing writes none of its own: what it lists is a fact
        // about the plan and travels on the page itself, because a listing with
        // a template draws its entries from the template and those links are
        // the template's, chrome like any other.
        // ...and only while something reads the graph: with neither backlinks
        // nor the orphan report asked for, a page pays neither the check below
        // nor a manifest field per build.
        let content = (page.authored() && config.links.graph()).then_some(cx.content);
        let found = &mut cx.found;
        doc.walk(|element| {
            let span = element.span;
            // A fragment is a deep link only where a reader can follow it.
            let anchor = element.tag == tag::a;
            // Asked only of a link that named a page, and so of a handful of
            // elements rather than of every node the page is made of.
            let authored = || {
                content
                    .as_ref()
                    .is_some_and(|dir| origins.authored(span, dir, page))
            };
            element.rewrite(&[attr::href, attr::src], |value| {
                // A bare `#fragment` names a section of *this* page. It is left
                // exactly as authored - it is already the URL a browser will
                // follow - but it is still a link into a heading, and only the
                // site-wide check knows which ids a page ended up with, so it
                // is recorded like any other deep link. Here rather than in
                // resolution, because this is the only place that knows which
                // page the link was written on.
                if anchor && let Some(target) = Self::fragment(value, &page.permalink) {
                    found.deep.push(target);
                    return None;
                }
                let resolution = links.classify(value, &page.source, lang);
                // Every probe becomes a dependency, whether or not it matched,
                // so the page rebuilds when the URL layout it resolved against
                // changes underneath it.
                found.links.extend(resolution.probed);
                match resolution.link {
                    Link::Resolved(target) => {
                        let url = target.to_string();
                        // A link naming a fragment of another page is checked
                        // site-wide once every page's anchors are known: the
                        // target's headings are not resolvable from here.
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
                        // A link already written as a URL still reaches a page:
                        // an author writing `/guide/`, and every generated index,
                        // which links its members by permalink because it has no
                        // source path to name them by.
                        if authored() {
                            let serving = links.served(value);
                            // Recorded whether or not it named a page, so a page
                            // appearing at that URL later rebuilds the linker
                            // and gains its backlink.
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
        match target.fragment().is_some() {
            true => Some(target),
            false => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Links;

    /// A fragment written on `/posts/a/` names a section of `/posts/a/`, so the
    /// site-wide check has a page to look the heading up on. It had none: a
    /// bare fragment was classified as a link off the site and was never
    /// checked at all, whatever `engine::check` claimed.
    #[test]
    fn a_bare_fragment_targets_the_page_it_was_written_on() {
        let target = Links::fragment("#install", "/posts/a/").expect("a fragment");

        assert_eq!(target.page(), "/posts/a/");
        assert_eq!(target.fragment(), Some("install"));
        // ...and it is still served exactly as the link will read once the
        // reader is on the page.
        assert_eq!(target.to_string(), "/posts/a/#install");
    }

    /// A query after the fragment is not part of it, which is the rule the
    /// target type already keeps; anything that is not a fragment, and a `#`
    /// that names no section, are not deep links.
    #[test]
    fn only_a_fragment_naming_a_section_is_a_deep_link() {
        let queried = Links::fragment("#install?x=1", "/a/").expect("a fragment");
        assert_eq!(queried.fragment(), Some("install"));

        for raw in ["#", "b.typ", "b.typ#install", "/a/#install", ""] {
            assert!(Links::fragment(raw, "/a/").is_none(), "{raw}");
        }
    }
}
