//! What a page ships, gathered from its DOM: the files it loads and the bytes
//! it inlines. Their sizes are not recorded here, since they change without the
//! page changing; [`crate::engine::check`] resolves those site-wide.

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
    /// The URL as the markup states it.
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
    /// The files it loads, in document order and deduplicated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loads: Vec<Reference>,
}

impl Weight {
    /// A page that inlines nothing stores nothing.
    // `skip_serializing_if` hands the field by reference, so serde chooses the
    // signature.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn unweighed(bytes: &u64) -> bool {
        *bytes == 0
    }

    /// A page that ships nothing beyond its markup, for a caller that needs a
    /// ledger to borrow.
    pub const EMPTY: &'static Self = &Self {
        js: 0,
        css: 0,
        loads: Vec::new(),
    };

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Weigh `doc`: one walk, reading what each element loads.
    pub(super) fn of(doc: &HtmlDocument) -> Self {
        let mut weight = Self::default();
        doc.visit(|element| weight.visit(element));
        weight
    }

    /// Read one element into the ledger. `srcset` candidates are not counted:
    /// they are alternatives to `src`, and a visitor is served exactly one.
    fn visit(&mut self, element: &HtmlElement) {
        match element.tag {
            tag::script => match element.attrs.get(attr::src) {
                Some(src) => self.load(Load::Js, src),
                None => self.js += element.text().len() as u64,
            },
            tag::style => self.css += element.text().len() as u64,
            _ if element.stylesheet() => {
                if let Some(href) = element.attrs.get(attr::href) {
                    self.load(Load::Css, href);
                }
            }
            tag::img | tag::source => {
                if let Some(src) = element.attrs.get(attr::src) {
                    self.load(Load::Image, src);
                }
            }
            _ => {}
        }
    }

    /// Record a load, unless this page already fetches that exact URL. A
    /// `data:` URI is skipped: its bytes are in the markup, already billed.
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
