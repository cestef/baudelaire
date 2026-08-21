//! Prefixes on-page root-absolute URLs with the site's base path.

use typst_html::HtmlDocument;

use crate::config::Config;

use super::{Cx, DocumentExt, Fingerprint, Transform};

/// Shifts every on-page root-absolute URL under the site's
/// [`base_path`](Config::base_path), for subdirectory hosting. Runs last, so it
/// sees final `href`/`src` values.
pub(super) struct BasePath;

impl BasePath {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "base";
}

impl Transform for BasePath {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Fingerprint::NAME]
    }

    fn enabled(&self, config: &Config) -> bool {
        !config.base_path().is_empty()
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let config = cx.config;
        doc.assets(|value| {
            let prefixed = config.prefixed(value);
            (prefixed != value).then_some(prefixed)
        });
    }
}
