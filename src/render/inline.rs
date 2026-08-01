//! The digests of what a page carries inline.
//!
//! A `Content-Security-Policy` strict enough to be worth having forbids inline
//! script and style outright, unless the policy names the exact bodies it
//! allows by digest. Nobody can maintain that list by hand: the bodies are
//! written by templates, by the speculation-rules pass, by the SPA runtime, and
//! they change whenever any of those do. They are collected here instead, while
//! the page is still a DOM, and the policy is assembled from what the build
//! actually produced.
//!
//! Recorded per page and stored with its other outputs, so a page served from
//! the cache contributes its digests exactly as a freshly compiled one does. A
//! page missing from that union is a page whose own scripts a browser refuses
//! to run.

use serde::{Deserialize, Serialize};

use crate::digest::Digest;

/// One page's inline digests, in the `sha256-..` spelling a policy carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inline {
    /// The digests of its `<script>` bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<String>,
    /// The digests of its `<style>` bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<String>,
    /// The digests of its `style=""` attributes.
    ///
    /// A style *attribute* is inline style too, and a policy that names only
    /// `<style>` elements drops every one of them. They are not baudelaire's to
    /// avoid, either: typst-html resolves an element's CSS properties into a
    /// `style` attribute, and so do the image show rule and the syntax
    /// highlighter. Kept apart from `styles` because allowing one takes
    /// `'unsafe-hashes'` beside the digest, which allowing a `<style>` does not.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
}

impl Inline {
    /// A page that inlines nothing, for a caller that needs one to borrow
    /// rather than one to own.
    pub const EMPTY: &'static Self = &Self {
        scripts: Vec::new(),
        styles: Vec::new(),
        attrs: Vec::new(),
    };

    /// Whether the page inlines nothing, so a manifest for a site with no
    /// policy is not one field heavier per page.
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty() && self.styles.is_empty() && self.attrs.is_empty()
    }

    /// Record a `<script>` body.
    pub fn script(&mut self, body: &str) {
        Self::add(&mut self.scripts, body);
    }

    /// Record a `<style>` body.
    pub fn style(&mut self, body: &str) {
        Self::add(&mut self.styles, body);
    }

    /// Record a `style=""` attribute's value.
    pub fn attr(&mut self, value: &str) {
        Self::add(&mut self.attrs, value);
    }

    /// Digest `body` into `into`, unless it is already there.
    ///
    /// The digest is over the bytes *between* the tags, exactly as a browser
    /// takes it, so an empty body is skipped: it is not something a policy has
    /// to allow, and hashing it would put a digest of `""` in every policy.
    fn add(into: &mut Vec<String>, body: &str) {
        if body.is_empty() {
            return;
        }
        let digest = Digest::sha256(body.as_bytes()).as_str().to_owned();
        if !into.contains(&digest) {
            into.push(digest);
        }
    }
}
