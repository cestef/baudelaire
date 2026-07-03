//! Inlines local assets referenced by a page as `data:` URIs.
//!
//! When `html { embed true }` is set, root-relative asset links (`href`/`src`
//! pointing at `/<assets>/…`) are replaced with a self-contained `data:` URI so
//! the page carries its own CSS/images/fonts. Best-effort: anything that is not
//! a resolvable local asset (external URLs, missing files) is left as authored.

use std::path::Path;

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr};

use crate::config::Config;

use super::transform::{Cx, Transform};

/// The [`Transform`] that rewrites local asset references to `data:` URIs.
pub(super) struct Embed;

impl Transform for Embed {
    fn enabled(&self, config: &Config) -> bool {
        config.html.embed
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        Inliner::new(cx.config).visit(doc.root_mut());
    }
}

/// Walks the element tree replacing resolvable local `href`/`src` values with
/// `data:` URIs.
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

    fn visit(&self, element: &mut HtmlElement) {
        for key in [attr::href, attr::src] {
            if let Some(value) = element.attrs.get_mut(key)
                && let Some(uri) = self.inline(value)
            {
                *value = uri.into();
            }
        }
        for child in element.children.make_mut() {
            if let HtmlNode::Element(child) = child {
                self.visit(child);
            }
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
        Some(format!("data:{};base64,{}", mime(&path), base64(&bytes)))
    }
}

/// The MIME type for an asset, by extension. Unknown types fall back to a
/// generic binary type, which browsers still accept in a `data:` URI.
fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("css") => "text/css",
        Some("js" | "mjs") => "text/javascript",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("json") => "application/json",
        _ => "application/octet-stream",
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
