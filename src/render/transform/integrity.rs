//! Subresource integrity, and the digests a content security policy needs.
//!
//! One walk, two halves of the same idea: what a page loads and what it
//! inlines, each named by its digest. A `<script src>` gets an `integrity` a
//! browser checks after fetching it; an inline `<script>` is never fetched, so
//! its digest goes into the policy instead ([`crate::render::inline`]), which
//! is what lets a page keep its inline blocks under a policy that forbids
//! inline script in general.
//!
//! Only files *this build wrote* are stamped. A script on someone else's host
//! would need `crossorigin` and a digest of bytes this build never saw, and
//! stamping a guess would block the resource rather than protect it.

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
            // On any element, not just the ones above: a `style` attribute is
            // inline style, a policy has to name it by digest, and this build
            // emits them all over (typst resolves an element's CSS properties
            // into one, as do the image rule and the highlighter).
            if hashes && let Some(style) = element.attrs.get(attr::style) {
                inline.attr(style);
            }
        });
        cx.found.inline = inline;
    }
}

impl Integrity {
    /// Stamp the digest of the file `key` points at, if this build wrote it.
    ///
    /// An element that already carries an `integrity` is left alone: the author
    /// pinned a specific digest, and overwriting it would silently undo the one
    /// thing the attribute is for.
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
