//! The digests of what a page carries inline, collected while it is still a DOM
//! so the generated `Content-Security-Policy` can name each body it allows.

use serde::{Deserialize, Serialize};

use crate::digest::Digest;

/// One page's inline digests, in the `sha256-..` spelling a policy carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inline {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<String>,
    /// The digests of its `style=""` attributes, kept apart from `styles`
    /// because allowing one takes `'unsafe-hashes'` beside the digest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
}

impl Inline {
    /// A page that inlines nothing, for a caller that needs one to borrow.
    pub const EMPTY: &'static Self = &Self {
        scripts: Vec::new(),
        styles: Vec::new(),
        attrs: Vec::new(),
    };

    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty() && self.styles.is_empty() && self.attrs.is_empty()
    }

    pub fn script(&mut self, body: &str) {
        Self::add(&mut self.scripts, body);
    }

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
    /// takes it; an empty body is nothing a policy has to allow, and is
    /// skipped.
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
