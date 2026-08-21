//! One extension-to-MIME-type table for every consumer (`data:` URIs, the dev
//! server's `Content-Type`): a single source so the types can't drift apart.

use std::fmt;
use std::path::Path;

/// The MIME type of a file, guessed from its extension. Unknown extensions fall
/// back to a generic binary type, which every consumer accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mime(&'static str);

impl Mime {
    /// What an unrecognized extension, or none at all, is served as.
    const BINARY: &'static str = "application/octet-stream";

    /// The type a page's PDF is served and advertised as.
    pub const PDF: &'static str = "application/pdf";

    /// The type an RSS feed is announced under.
    pub const RSS: &'static str = "application/rss+xml";

    /// The type an Atom feed is announced under.
    pub const ATOM: &'static str = "application/atom+xml";

    /// The type a JSON Feed is announced under.
    pub const JSON_FEED: &'static str = "application/feed+json";

    /// The type an EPUB is served and written into the book's own manifest as.
    pub const EPUB: &'static str = "application/epub+zip";

    /// The MIME type named by `path`'s extension, matched case-insensitively.
    pub fn of(path: impl AsRef<Path>) -> Self {
        let Some(ext) = path.as_ref().extension().and_then(|e| e.to_str()) else {
            return Self(Self::BINARY);
        };
        if let Some(format) = ImageFormat::from_ext(ext) {
            return format.mime();
        }
        Self(match ext.to_ascii_lowercase().as_str() {
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" | "mjs" => "text/javascript",
            "svg" => "image/svg+xml",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "avif" => "image/avif",
            "ico" => "image/x-icon",
            "woff2" => "font/woff2",
            "woff" => "font/woff",
            "ttf" => "font/ttf",
            "otf" => "font/otf",
            "json" | "map" => "application/json",
            "webmanifest" => "application/manifest+json",
            "xml" => "application/xml",
            "txt" => "text/plain",
            "pdf" => Self::PDF,
            "epub" => Self::EPUB,
            _ => Self::BINARY,
        })
    }

    /// The `Content-Type` header value: the type, plus a UTF-8 charset for the
    /// types that register one.
    ///
    /// Not every type a human would call textual: XML registers a `charset`
    /// parameter (RFC 7303) and JSON registers none (RFC 8259).
    pub fn header(self) -> String {
        let charset =
            self.0.starts_with("text/") || self.0 == "application/xml" || self.0.ends_with("+xml");
        if charset {
            format!("{}; charset=utf-8", self.0)
        } else {
            self.0.to_owned()
        }
    }

    /// Whether this is HTML: the dev server injects its live-reload client into
    /// exactly these responses.
    pub fn html(self) -> bool {
        self.0 == "text/html"
    }
}

impl fmt::Display for Mime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A raster format the asset pipeline can decode and re-encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    /// Each format's media type and the extensions naming it, read by both
    /// [`from_ext`](Self::from_ext) and [`Mime::of`].
    const FORMATS: &'static [(Self, &'static str, &'static [&'static str])] = &[
        (Self::Png, "image/png", &["png"]),
        (Self::Jpeg, "image/jpeg", &["jpg", "jpeg", "jpe", "jfif"]),
    ];

    /// The raster format a file extension names, matched case-insensitively,
    /// and `None` when unrecognized.
    pub fn from_ext(ext: &str) -> Option<Self> {
        let ext = ext.to_ascii_lowercase();
        Self::FORMATS
            .iter()
            .find(|(_, _, exts)| exts.contains(&ext.as_str()))
            .map(|(format, ..)| *format)
    }

    pub fn mime(self) -> Mime {
        Mime(
            Self::FORMATS
                .iter()
                .find(|(format, ..)| *format == self)
                .map_or(Mime::BINARY, |(_, mime, _)| *mime),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{ImageFormat, Mime};

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
            ("book.epub", "application/epub+zip"),
            ("paper.pdf", "application/pdf"),
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
    }

    #[test]
    fn extensions_match_regardless_of_case() {
        assert_eq!(Mime::of("INDEX.HTML").to_string(), "text/html");
        assert_eq!(Mime::of("Photo.PNG").to_string(), "image/png");
        assert_eq!(Mime::of("Style.CSS").to_string(), "text/css");
    }

    #[test]
    fn every_raster_extension_agrees_with_the_optimizer() {
        for ext in ["png", "jpg", "jpeg", "jpe", "jfif", "JFIF"] {
            let format = ImageFormat::from_ext(ext).unwrap_or_else(|| panic!("{ext} is a raster"));
            assert_eq!(
                Mime::of(format!("pic.{ext}")),
                format.mime(),
                "{ext} served as something other than what it is optimized as"
            );
        }
        assert_eq!(Mime::of("pic.jfif").to_string(), "image/jpeg");
        assert_eq!(Mime::of("pic.jpe").to_string(), "image/jpeg");
        assert_eq!(ImageFormat::from_ext("svg"), None);
    }

    #[test]
    fn header_adds_charset_to_the_types_that_define_one() {
        assert_eq!(Mime::of("i.html").header(), "text/html; charset=utf-8");
        assert_eq!(Mime::of("a.js").header(), "text/javascript; charset=utf-8");
        assert_eq!(Mime::of("n.txt").header(), "text/plain; charset=utf-8");
        assert_eq!(Mime::of("f.xml").header(), "application/xml; charset=utf-8");
        assert_eq!(Mime::of("s.svg").header(), "image/svg+xml; charset=utf-8");
        assert_eq!(Mime::of("d.json").header(), "application/json");
        assert_eq!(
            Mime::of("m.webmanifest").header(),
            "application/manifest+json"
        );
        assert_eq!(Mime::of("p.png").header(), "image/png");
    }

    #[test]
    fn html_predicate_is_exact() {
        assert!(Mime::of("page.html").html());
        assert!(!Mime::of("style.css").html());
        assert!(!Mime::of("logo.svg").html());
        assert!(!Mime::of("unknown.bin").html());
    }
}
