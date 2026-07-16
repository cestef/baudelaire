//! One extension-to-MIME-type table for every consumer (`data:` URIs, the dev
//! server's `Content-Type`): a single source so the types can't drift apart.

use std::fmt;
use std::path::Path;

/// The MIME type of a file, guessed from its extension. Unknown extensions fall
/// back to a generic binary type, which every consumer accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mime(&'static str);

impl Mime {
    pub fn of(path: impl AsRef<Path>) -> Self {
        Self(match path.as_ref().extension().and_then(|e| e.to_str()) {
            Some("html") => "text/html",
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
            Some("xml") => "application/xml",
            Some("txt") => "text/plain",
            _ => "application/octet-stream",
        })
    }

    /// The `Content-Type` header value: the type, plus a UTF-8 charset for
    /// textual types.
    pub fn header(self) -> String {
        match self.0.starts_with("text/") {
            true => format!("{}; charset=utf-8", self.0),
            false => self.0.to_owned(),
        }
    }

    /// Whether this is HTML: the dev server injects its live-reload client
    /// into exactly these responses.
    pub fn html(self) -> bool {
        self.0 == "text/html"
    }
}

impl fmt::Display for Mime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
