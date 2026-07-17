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

#[cfg(test)]
mod tests {
    use super::Mime;

    #[test]
    fn maps_known_extensions() {
        let cases = [
            ("index.html", "text/html"),
            ("style.css", "text/css"),
            ("app.js", "text/javascript"),
            ("mod.mjs", "text/javascript"),
            ("logo.svg", "image/svg+xml"),
            ("pic.png", "image/png"),
            ("pic.jpg", "image/jpeg"),
            ("pic.jpeg", "image/jpeg"),
            ("anim.gif", "image/gif"),
            ("pic.webp", "image/webp"),
            ("pic.avif", "image/avif"),
            ("fav.ico", "image/x-icon"),
            ("f.woff2", "font/woff2"),
            ("f.woff", "font/woff"),
            ("f.ttf", "font/ttf"),
            ("data.json", "application/json"),
            ("feed.xml", "application/xml"),
            ("notes.txt", "text/plain"),
        ];
        for (path, want) in cases {
            assert_eq!(Mime::of(path).to_string(), want, "{path}");
        }
    }

    #[test]
    fn unknown_and_missing_extension_fall_back_to_binary() {
        assert_eq!(
            Mime::of("archive.tar.zst").to_string(),
            "application/octet-stream"
        );
        assert_eq!(Mime::of("Makefile").to_string(), "application/octet-stream");
        // Matching is case-sensitive: an uppercase extension is not recognized.
        assert_eq!(
            Mime::of("INDEX.HTML").to_string(),
            "application/octet-stream"
        );
    }

    #[test]
    fn header_adds_charset_only_for_text() {
        assert_eq!(Mime::of("i.html").header(), "text/html; charset=utf-8");
        assert_eq!(Mime::of("a.js").header(), "text/javascript; charset=utf-8");
        assert_eq!(Mime::of("n.txt").header(), "text/plain; charset=utf-8");
        // Non-text types carry no charset.
        assert_eq!(Mime::of("p.png").header(), "image/png");
        assert_eq!(Mime::of("d.json").header(), "application/json");
        assert_eq!(Mime::of("s.svg").header(), "image/svg+xml");
    }

    #[test]
    fn html_predicate_is_exact() {
        assert!(Mime::of("page.html").html());
        assert!(!Mime::of("style.css").html());
        assert!(!Mime::of("logo.svg").html());
        assert!(!Mime::of("unknown.bin").html());
    }
}
