//! Inlines local assets referenced by a page as `data:` URIs, under
//! `html { embed true }`. Best-effort: anything that does not resolve to a
//! local asset is left as authored.

use std::path::PathBuf;

use typst_html::{HtmlDocument, tag};

use crate::config::Config;
use crate::digest::Base64;
use crate::mime::Mime;

use super::{Cx, DocumentExt, ElementExt, Exempt, Externalize, Sources, Transform};
use crate::render::{AssetDeps, AssetMap};

/// The [`Transform`] that rewrites local asset references to `data:` URIs.
pub(super) struct Embed;

impl Embed {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "embed";
}

impl Transform for Embed {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME, Externalize::NAME, Sources::NAME]
    }

    fn enabled(&self, config: &Config) -> bool {
        config.html.embed
    }

    /// A `<meta>` is skipped: its URLs are fetched by a scraper that was
    /// handed the URL, and a `data:` URI is nothing it can retrieve.
    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut inliner = Inliner::new(cx.config, cx.assets);
        doc.walk(|element| {
            if element.tag == tag::meta {
                return;
            }
            element.assets(|value| inliner.inline(value));
        });
        cx.found.assets.extend(inliner.probed);
        cx.found.read.extend(inliner.inlined);
    }
}

/// Resolves local `href`/`src` values to `data:` URIs over the *processed*
/// asset under `dist`, not the raw source, so an embedded asset carries the
/// same bytes a linked one would serve.
struct Inliner<'a> {
    dst: PathBuf,
    /// The leading URL segment a reference must start with to be a local asset,
    /// e.g. `/assets/`.
    prefix: String,
    assets: &'a AssetMap,
    /// The map entries this page's embedded references consulted.
    probed: AssetDeps,
    inlined: Vec<PathBuf>,
}

impl<'a> Inliner<'a> {
    fn new(config: &Config, assets: &'a AssetMap) -> Self {
        Self {
            probed: AssetDeps::new(),
            inlined: Vec::new(),
            dst: config.asset_staging(),
            prefix: format!("/{}/", config.asset_name()),
            assets,
        }
    }

    /// The `data:` URI for a local asset reference, `None` to leave it as is.
    ///
    /// A path is recorded before it is read, not after: a reference to a file
    /// that was not there has to invalidate when it appears.
    fn inline(&mut self, raw: &str) -> Option<String> {
        let resolved = self.assets.resolve(raw);
        self.probed.extend(resolved.probed);
        let served = resolved.url.unwrap_or_else(|| raw.to_owned());
        let rest = served.strip_prefix(&self.prefix)?;
        if rest.contains("..") || rest.contains(['?', '#']) {
            return None;
        }
        let path = self.dst.join(rest);
        self.inlined.push(path.clone());
        let bytes = crate::fs::read(&path).ok()?;
        Some(format!(
            "data:{};base64,{}",
            Mime::of(&path),
            Base64(&bytes)
        ))
    }
}
