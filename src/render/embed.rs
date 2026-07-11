//! Inlines local assets referenced by a page as `data:` URIs.
//!
//! When `html { embed true }` is set, root-relative asset links (`href`/`src`
//! pointing at `/<assets>/…`) are replaced with a self-contained `data:` URI so
//! the page carries its own CSS/images/fonts. Best-effort: anything that is not
//! a resolvable local asset (external URLs, missing files) is left as authored.

use std::path::Path;

use typst_html::{HtmlDocument, attr};

use crate::config::Config;
use crate::mime::Mime;

use super::transform::{Cx, ElementExt, Transform};

/// The [`Transform`] that rewrites local asset references to `data:` URIs.
pub(super) struct Embed;

impl Transform for Embed {
    fn enabled(&self, config: &Config) -> bool {
        config.html.embed
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let inliner = Inliner::new(cx.config);
        doc.root_mut().walk(&mut |element| {
            element.rewrite(&[attr::href, attr::src], |value| inliner.inline(value));
        });
    }
}

/// Resolves local `href`/`src` values to `data:` URIs.
struct Inliner<'a> {
    assets: &'a Path,
    /// The leading URL segment that maps to the assets directory, e.g.
    /// `/assets/`. Refs must start with it to be considered local assets.
    prefix: Option<String>,
}

impl<'a> Inliner<'a> {
    fn new(config: &'a Config) -> Self {
        let prefix = config
            .assets
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("/{name}/"));
        Self {
            assets: &config.assets,
            prefix,
        }
    }

    /// The `data:` URI for a local asset reference, or `None` to leave it as is.
    fn inline(&self, raw: &str) -> Option<String> {
        let rest = raw.strip_prefix(self.prefix.as_deref()?)?;
        // Reject anything that escapes the assets directory or carries a
        // query/fragment — those are not plain file references.
        if rest.contains("..") || rest.contains(['?', '#']) {
            return None;
        }
        let path = self.assets.join(rest);
        let bytes = std::fs::read(&path).ok()?;
        Some(format!("data:{};base64,{}", Mime::of(&path), base64(&bytes)))
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
