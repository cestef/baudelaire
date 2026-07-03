//! Rewrites asset references to their content-addressed (fingerprinted) URLs.
//!
//! When `output { assets { fingerprint #true } }` is set, the engine copies each
//! asset to a content-hashed name (`style.css` → `style.<hash>.css`) and records
//! the mapping in an [`super::AssetMap`]. This transform rewrites every `href`/
//! `src` that names an original asset to its hashed URL, so caches can serve
//! assets forever and bust automatically on change.

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr};

use crate::config::Config;

use super::AssetMap;
use super::transform::{Cx, Transform};

/// The [`Transform`] that swaps asset references for their fingerprinted URLs.
pub(super) struct Fingerprint;

impl Transform for Fingerprint {
    fn enabled(&self, config: &Config) -> bool {
        config.asset.fingerprint
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        Rewriter { assets: cx.assets }.visit(doc.root_mut());
    }
}

/// Walks the element tree replacing mapped `href`/`src` values with their
/// fingerprinted URLs. Anything not in the map (external URLs, already-inlined
/// `data:` URIs, unmanaged paths) is left untouched.
struct Rewriter<'a> {
    assets: &'a AssetMap,
}

impl Rewriter<'_> {
    fn visit(&self, element: &mut HtmlElement) {
        for key in [attr::href, attr::src] {
            if let Some(value) = element.attrs.get_mut(key)
                && let Some(url) = self.assets.resolve(value)
            {
                *value = url.into();
            }
        }
        for child in element.children.make_mut() {
            if let HtmlNode::Element(child) = child {
                self.visit(child);
            }
        }
    }
}
