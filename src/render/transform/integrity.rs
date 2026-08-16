//! Subresource integrity, and the digests a content security policy needs: a
//! fetched resource gets an `integrity`, an inline one's digest goes into the
//! policy. Only files this build wrote are stamped.

use typst_html::{HtmlDocument, HtmlElement, attr, tag};

use crate::config::Config;
use crate::render::Emitted;

use super::{Cx, DocumentExt, ElementExt, Transform};

/// The [`Transform`] that stamps `integrity` and collects inline digests.
pub(super) struct Integrity;

impl Transform for Integrity {
    /// Either half is reason enough to walk: a site may stamp integrity without
    /// generating a policy, or generate one without stamping anything.
    fn enabled(&self, config: &Config) -> bool {
        config.sri() || config.hashes()
    }

    /// A `style` attribute is checked on every element, not just the ones the
    /// match names, since a policy has to name each one by digest.
    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let sri = cx.config.sri();
        let hashes = cx.config.hashes();
        let emitted = cx.emitted;
        let mut inline = crate::render::Inline::default();
        doc.walk(|element| {
            match element.tag {
                tag::script => match element.attrs.get(attr::src) {
                    Some(_) if sri => Self::stamp(element, attr::src, emitted),
                    None if hashes => inline.script(&element.text()),
                    _ => {}
                },
                tag::style if hashes => inline.style(&element.text()),
                _ if sri && element.stylesheet() => Self::stamp(element, attr::href, emitted),
                _ => {}
            }
            if hashes && let Some(style) = element.attrs.get(attr::style) {
                inline.attr(style);
            }
        });
        cx.found.inline = inline;
    }
}

impl Integrity {
    /// Stamp the digest of the file `key` points at, if this build wrote it and
    /// the author has not already pinned an `integrity`.
    fn stamp(element: &mut HtmlElement, key: typst_html::HtmlAttr, emitted: &Emitted) {
        if element.attrs.get(attr::integrity).is_some() {
            return;
        }
        let Some(digest) = element
            .attrs
            .get(key)
            .and_then(|url| emitted.at(url))
            .and_then(|file| file.digest.as_ref())
        else {
            return;
        };
        element.set(attr::integrity, digest.as_str());
    }
}
