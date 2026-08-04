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
        // A generated listing writes none of its own: what it lists is a fact
        // about the plan and travels on the page itself (`Data::Generated`),
        // because a listing with a template draws its entries from the template
        // and those links are the template's, chrome like any other.
        // ...and only while something reads the graph: with neither backlinks
        // nor the orphan report asked for, a page pays neither the check below
        // nor a manifest field per build.
        let content = match page.data {
            Data::Generated { .. } => None,
            _ => config.links.graph().then_some(*content),
        };
        doc.walk(|element| {
            let span = element.span;
            // Asked only of a link that named a page, and so of a handful of
            // elements rather than of every node the page is made of.
            let authored = || {
                content
                    .as_ref()
                    .is_some_and(|dir| Origins::authored_under(span, dir))
            };
            element.rewrite(&[attr::href, attr::src], |value| {
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
                        if authored()
                            && let Some(target) = links.served(value)
                        {
                            found.outbound.record(target, &page.permalink);
                        }
                        None
                    }
                }
            });
        });
    }
}
