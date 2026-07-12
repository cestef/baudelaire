//! Rewrites asset references to their content-addressed (fingerprinted) URLs.
//!
//! When `output { assets { fingerprint #true } }` is set, the engine copies each
//! asset to a content-hashed name (`style.css` → `style.<hash>.css`) and records
//! the mapping in an [`super::AssetMap`]. This transform rewrites every `href`/
//! `src` that names an original asset to its hashed URL, so caches can serve
//! assets forever and bust automatically on change.

use typst_html::{HtmlDocument, attr};

use crate::config::Config;

use super::transform::{Cx, ElementExt, Transform};

/// The [`Transform`] that swaps asset references for their fingerprinted URLs.
///
/// Replaces mapped `href`/`src`/`content`/`poster`/`srcset` values with their
/// fingerprinted URLs. `content` covers asset references in `<meta>` tags (a
/// social `og:image`); `srcset` covers responsive `<img>`/`<source>` candidate
/// lists. Anything not in the map (external URLs, already-inlined `data:` URIs,
/// unmanaged paths, plain text) is left untouched.
pub(super) struct Fingerprint;

impl Transform for Fingerprint {
    fn enabled(&self, config: &Config) -> bool {
        config.asset.fingerprint
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        doc.root_mut().walk(&mut |element| {
            element.assets(&[attr::href, attr::src, attr::content, attr::poster], |value| {
                cx.assets.resolve(value)
            });
        });
    }
}
