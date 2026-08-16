//! Rewrites asset references to their content-addressed (fingerprinted) URLs.

use typst_html::HtmlDocument;

use crate::config::Config;

use super::{Cx, DocumentExt, Transform};

/// The [`Transform`] that swaps mapped asset references for their fingerprinted
/// URLs, leaving anything the map does not name untouched.
pub(super) struct Fingerprint;

impl Transform for Fingerprint {
    fn enabled(&self, config: &Config) -> bool {
        config.assets.fingerprint
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let Cx { assets, found, .. } = cx;
        doc.assets(|value| {
            let served = assets.resolve(value);
            found.assets.extend(served.probed);
            served.url
        });
    }
}
