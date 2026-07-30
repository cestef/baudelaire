//! Structure-aware link rewriting over typst's own HTML DOM.

use typst_html::{HtmlDocument, attr};

use crate::config::Config;
use crate::render::links::Link;
use crate::render::transform::{Cx, DocumentExt, ElementExt, Transform};

/// The core [`Transform`]: resolves internal `.typ` source-path links to
/// permalinks. Always runs (it is URL resolution, not an optional pass) and
/// records, in the context, both the broken internal links (for link checking)
/// and every map entry it consulted (for the build cache). Operates on the
/// typed DOM (never on the serialized string) so it can't corrupt markup.
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
            found,
            ..
        } = cx;
        let lang = config.multilingual().then_some(page.lang.as_str());
        doc.walk(|element| {
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
