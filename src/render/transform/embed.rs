//! Inlines local assets referenced by a page as `data:` URIs.
//!
//! When `html { embed true }` is set, root-relative asset links (`href`/`src`
//! pointing at `/<assets>/..`) are replaced with a self-contained `data:` URI so
//! the page carries its own CSS/images/fonts. Best-effort: anything that is not
//! a resolvable local asset (external URLs, missing files) is left as authored.

use std::path::PathBuf;

use typst_html::HtmlDocument;

use crate::config::Config;
use crate::digest::Base64;
use crate::mime::Mime;

use super::{Cx, DocumentExt, Transform};
use crate::render::{AssetDeps, AssetMap};

/// The [`Transform`] that rewrites local asset references to `data:` URIs.
pub(super) struct Embed;

impl Transform for Embed {
    fn enabled(&self, config: &Config) -> bool {
        config.html.embed
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut inliner = Inliner::new(cx.config, cx.assets);
        doc.assets(|value| inliner.inline(value));
        // The inliner resolves through the same map the fingerprint transform
        // does, so what it looked up is a dependency of this page too, and the
        // files whose bytes it inlined are dependencies in the ordinary sense.
        cx.found.assets.extend(inliner.probed);
        cx.found.read.extend(inliner.inlined);
    }
}

/// Resolves local `href`/`src` values to `data:` URIs over the *processed*
/// asset: the minified/bundled/optimized (and possibly fingerprinted) output
/// under `dist`, not the raw source, so an embedded asset carries the same
/// bytes a linked one would serve.
struct Inliner<'a> {
    /// Destination asset directory under `dist` (e.g. `dist/assets`).
    dst: PathBuf,
    /// The leading URL segment that maps to the assets directory, e.g.
    /// `/assets/`. Refs must start with it to be considered local assets.
    prefix: String,
    /// Request-to-served URL map, so a fingerprinted reference resolves to its
    /// hashed output file rather than a name no longer present in `dist`.
    assets: &'a AssetMap,
    /// The map entries this page's embedded references consulted.
    probed: AssetDeps,
    /// The processed files whose bytes were inlined. Their contents are in the
    /// page's markup, so an edit to one has to rebuild it; as ordinary
    /// dependencies they ride the same content-hash check as every other file.
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

    /// The `data:` URI for a local asset reference, or `None` to leave it as is.
    fn inline(&mut self, raw: &str) -> Option<String> {
        // a fingerprinted ref resolves to its hashed file; unmapped ones keep their name
        let resolved = self.assets.resolve(raw);
        self.probed.extend(resolved.probed);
        let served = resolved.url.unwrap_or_else(|| raw.to_owned());
        let rest = served.strip_prefix(&self.prefix)?;
        // reject dir escapes and query/fragment refs, not plain file references
        if rest.contains("..") || rest.contains(['?', '#']) {
            return None;
        }
        let path = self.dst.join(rest);
        // Recorded before the read, not after: a file that was not there is
        // exactly the case the page has to rebuild for. `Rewrite::read` records
        // an unhashable path as `None`, which its later appearance invalidates,
        // and pushing only on success left a page that referenced a missing
        // asset a cache hit for ever, still claiming to be self-contained while
        // pointing at a file it does not carry.
        self.inlined.push(path.clone());
        // best-effort: an unreadable asset stays a plain reference
        let bytes = crate::fs::read(&path).ok()?;
        Some(format!(
            "data:{};base64,{}",
            Mime::of(&path),
            Base64(&bytes)
        ))
    }
}
