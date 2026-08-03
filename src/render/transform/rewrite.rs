//! Structure-aware link rewriting over typst's own HTML DOM.

use typst_html::{HtmlDocument, attr};

use crate::config::Config;
use crate::content::Data;
use crate::render::links::Link;
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
        let Cx {
            links,
            page,
            config,
            content,
            found,
            ..
        } = cx;
        let lang = config.multilingual().then_some(page.lang.as_str());
        // Which of this page's links are *its own*, rather than its layout's.
        // A generated listing links every page it lists, and a term page every
        // page carrying the term, neither of which the author wrote: counted as
        // edges they would drown out the ones that were.
        // ...and only while something reads them: with `links { backlinks }` off
        // nothing inverts the graph, so a page pays neither the check below nor
        // a manifest field per build.
        let content = match page.data {
            Data::Generated(_) => None,
            _ => config.links.backlinks.then_some(*content),
        };
        doc.walk(|element| {
            let span = element.span;
            element.rewrite(&[attr::href, attr::src], |value| {
                let resolution = links.classify(value, &page.source, lang);
                // Every probe becomes a dependency, whether or not it matched,
                // so the page rebuilds when the URL layout it resolved against
                // changes underneath it.
                found.links.extend(resolution.probed);
                match resolution.link {
                    Link::Resolved(url) => {
                        // A link naming a fragment of another page is checked
                        // site-wide once every page's anchors are known: the
                        // target's headings are not resolvable from here.
                        if url.contains('#') {
                            found.deep.push(url.clone());
                        }
                        // Asked only of a link that resolved, and so of a
                        // handful of elements rather than of every node the
                        // page is made of.
                        if content
                            .as_ref()
                            .is_some_and(|dir| Origins::authored_under(span, dir))
                        {
                            found.outbound.record(&url, &page.permalink);
                        }
                        Some(url)
                    }
                    Link::Broken => {
                        found.broken.push(value.to_owned());
                        None
                    }
                    Link::Passthrough => None,
                }
            });
        });
    }
}
