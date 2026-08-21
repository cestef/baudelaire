//! Rewrites asset references to their content-addressed (fingerprinted) URLs.

use typst_html::HtmlDocument;

use crate::config::Config;

use super::{
    Cx, DocumentExt, Embed, Exempt, Externalize, Links, Math, Meta, Sheets, Sources, Svg, Transform,
};

/// The [`Transform`] that swaps mapped asset references for their fingerprinted
/// URLs, leaving anything the map does not name untouched.
pub(super) struct Fingerprint;

impl Fingerprint {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "fingerprint";
}

impl Transform for Fingerprint {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[
            Exempt::NAME,
            Links::NAME,
            Svg::NAME,
            Meta::NAME,
            Math::NAME,
            Sheets::NAME,
            Externalize::NAME,
            Sources::NAME,
            Embed::NAME,
        ]
    }

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
