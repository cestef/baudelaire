//! What a page ships, gathered from its DOM.
//!
//! A budget is about transfer weight, and only the page knows what it loads:
//! which scripts, which stylesheets, which images, and how many bytes of those
//! it carries inline. That much is recorded here, per page, and stored with the
//! page's other outputs so a cache hit is weighed exactly like a fresh compile.
//!
//! What is *not* recorded is how large each of those files is. That belongs to
//! the build that emitted them, changes without the page changing, and is
//! resolved site-wide in [`crate::engine::check`] against
//! [`crate::render::Emitted`]. Storing the sizes here would freeze a page's
//! verdict at the moment it compiled, which is exactly how a budget stops
//! catching anything.

use serde::{Deserialize, Serialize};
use typst_html::{HtmlDocument, HtmlElement, attr, tag};

use crate::render::transform::{DocumentExt, ElementExt};

/// What kind of file a reference loads, which is the budget it counts against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Load {
    Js,
    Css,
    Image,
}

/// One file a page loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub load: Load,
    /// The URL as the markup states it, which is what a browser would fetch.
    pub url: String,
}

/// One page's ledger: the bytes it carries inline, and the files it loads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weight {
    /// Bytes of inline `<script>` bodies.
    #[serde(default, skip_serializing_if = "Weight::unweighed")]
    pub js: u64,
    /// Bytes of inline `<style>` bodies.
    #[serde(default, skip_serializing_if = "Weight::unweighed")]
    pub css: u64,
    /// The files it loads, in document order and deduplicated: a stylesheet
    /// pulled in twice is fetched once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loads: Vec<Reference>,
}

impl Weight {
    /// A page that inlines nothing stores nothing; `serde` needs a path to
    /// call, and this one belongs to the type it is about.
    // `skip_serializing_if` hands the field by reference, so the signature is
    // serde's to choose and not ours.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn unweighed(bytes: &u64) -> bool {
        *bytes == 0
    }

    /// A page that ships nothing beyond its markup, for a caller that needs a
    /// ledger to borrow rather than one to own.
    pub const EMPTY: &'static Self = &Self {
        js: 0,
        css: 0,
        loads: Vec::new(),
    };

    /// Whether the page carries nothing worth storing, so a manifest written
    /// for a site with no budgets is not one field heavier per page.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Weigh `doc`: one walk, reading what each element loads.
    pub(super) fn of(doc: &HtmlDocument) -> Self {
        let mut weight = Self::default();
        doc.visit(|element| weight.visit(element));
        weight
    }

    /// Read one element into the ledger.
    fn visit(&mut self, element: &HtmlElement) {
        match element.tag {
            tag::script => match element.attrs.get(attr::src) {
                Some(src) => self.load(Load::Js, src),
                // An inline script ships with the markup, so its bytes are the
                // page's own; they are counted against `js` all the same,
                // because moving a script into the page does not make it free.
                None => self.js += element.text().len() as u64,
            },
            tag::style => self.css += element.text().len() as u64,
            _ if element.stylesheet() => {
                if let Some(href) = element.attrs.get(attr::href) {
                    self.load(Load::Css, href);
                }
            }
            // `srcset` candidates are deliberately not counted: they are
            // alternatives to `src`, and a visitor is served exactly one of
            // them. Counting the set would bill every page for the whole
            // responsive ladder.
            tag::img | tag::source => {
                if let Some(src) = element.attrs.get(attr::src) {
                    self.load(Load::Image, src);
                }
            }
            _ => {}
        }
    }

    /// Record a load, unless this page already fetches that exact URL.
    ///
    /// A `data:` URI is skipped: its bytes are in the markup, and `html` has
    /// already been billed for them.
    fn load(&mut self, load: Load, url: &str) {
        if url.starts_with("data:") || self.loads.iter().any(|seen| seen.url == url) {
            return;
        }
        self.loads.push(Reference {
            load,
            url: url.to_owned(),
        });
    }
}
