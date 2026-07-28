//! Structure-aware link rewriting over typst's own HTML DOM.

use typst_html::{HtmlDocument, attr};

use crate::config::Config;
use crate::render::links::Link;
use crate::render::transform::{Cx, DocumentExt, ElementExt, Transform};

/// The core [`Transform`]: resolves internal `.typ` source-path links to
/// permalinks. Always runs (it is URL resolution, not an optional pass) and
/// records broken internal links in the context for link checking. Operates on
/// the typed DOM (never on the serialized string) so it can't corrupt markup.
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
        let broken = &mut found.broken;
        let lang = config.multilingual().then_some(page.lang.as_str());
        doc.walk(|element| {
            element.rewrite(&[attr::href, attr::src], |value| {
                match links.classify(value, &page.source, lang) {
                    Link::Resolved(url) => Some(url),
                    Link::Broken => {
                        broken.push(value.to_owned());
                        None
                    }
                    Link::Passthrough => None,
                }
            });
        });
    }
}
