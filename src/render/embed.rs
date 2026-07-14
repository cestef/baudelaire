//! Inlines local assets referenced by a page as `data:` URIs.
//!
//! When `html { embed true }` is set, root-relative asset links (`href`/`src`
//! pointing at `/<assets>/..`) are replaced with a self-contained `data:` URI so
//! the page carries its own CSS/images/fonts. Best-effort: anything that is not
//! a resolvable local asset (external URLs, missing files) is left as authored.

use std::path::PathBuf;

use typst_html::{HtmlDocument, attr};

use crate::config::Config;
use crate::mime::Mime;

use super::AssetMap;
use super::transform::{Cx, ElementExt, Transform};

/// The [`Transform`] that rewrites local asset references to `data:` URIs.
pub(super) struct Embed;

impl Transform for Embed {
    fn enabled(&self, config: &Config) -> bool {
        config.html.embed
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let inliner = Inliner::new(cx.config, cx.assets);
        doc.root_mut().walk(&mut |element| {
            element.assets(&[attr::href, attr::src, attr::poster], |value| {
                inliner.inline(value)
            });
        });
    }
}

/// Resolves local `href`/`src` values to `data:` URIs over the *processed*
/// asset — the minified/bundled/optimized (and possibly fingerprinted) output
/// under `dist`, not the raw source — so an embedded asset carries the same
/// bytes a linked one would serve.
struct Inliner<'a> {
    /// Destination asset directory under `dist` (e.g. `dist/assets`).
    dst: PathBuf,
    /// The leading URL segment that maps to the assets directory, e.g.
    /// `/assets/`. Refs must start with it to be considered local assets.
    prefix: String,
    /// Request→served URL map, so a fingerprinted reference resolves to its
    /// hashed output file rather than a name no longer present in `dist`.
    assets: &'a AssetMap,
}

impl<'a> Inliner<'a> {
    fn new(config: &Config, assets: &'a AssetMap) -> Self {
        Self {
            dst: config.asset_dist(),
            prefix: format!("/{}/", config.asset_name()),
            assets,
        }
    }

    /// The `data:` URI for a local asset reference, or `None` to leave it as is.
    fn inline(&self, raw: &str) -> Option<String> {
        // a fingerprinted ref resolves to its hashed file; unmapped ones keep their name
        let served = self.assets.resolve(raw).unwrap_or_else(|| raw.to_owned());
        let rest = served.strip_prefix(&self.prefix)?;
        // reject dir escapes and query/fragment refs — not plain file references
        if rest.contains("..") || rest.contains(['?', '#']) {
            return None;
        }
        let path = self.dst.join(rest);
        // best-effort: an unreadable asset stays a plain reference
        let bytes = crate::fs::read(&path).ok()?;
        Some(format!(
            "data:{};base64,{}",
            Mime::of(&path),
            base64(&bytes)
        ))
    }
}

/// Standard (RFC 4648) base64 with `=` padding.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        out.push(TABLE[(bits >> 18 & 0x3f) as usize] as char);
        out.push(TABLE[(bits >> 12 & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(bits >> 6 & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(bits & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
