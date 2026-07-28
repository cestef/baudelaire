//! Prefixes on-page root-absolute URLs with the site's base path.

use typst_html::HtmlDocument;

use crate::config::Config;

use super::{Cx, DocumentExt, Transform};

/// The final [`Transform`]: shifts every on-page root-absolute URL under the
/// site's [`base_path`](Config::base_path) for subdirectory hosting. Runs after
/// fingerprinting and embedding, so it sees final `href`/`src` values and skips
/// already-inlined `data:` URIs and the absolute canonical/og URLs from
/// [`super::meta::Meta`] (neither is root-absolute).
pub(super) struct BasePath;

impl Transform for BasePath {
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
