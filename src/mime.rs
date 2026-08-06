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

    /// The type a page's PDF is served and advertised as. Named because two
    /// places state it: the extension table below, and the
    /// `<link rel="alternate">` the meta transform points at the file.
    pub const PDF: &'static str = "application/pdf";

    /// The MIME type named by `path`'s extension, matched case-insensitively:
    /// `Photo.PNG` names the same type as `photo.png`, as it must, since
    /// [`ImageFormat`] has always classified the two alike and a file the asset
    /// pipeline optimized as a PNG cannot then be served as a generic binary.
    pub fn of(path: impl AsRef<Path>) -> Self {
        let Some(ext) = path.as_ref().extension().and_then(|e| e.to_str()) else {
            return Self(Self::BINARY);
        };
        // Rasters resolve through `ImageFormat`, not through a second list of
        // extensions here: `.jfif` and `.jpe` were JPEG to the optimizer and
        // `application/octet-stream` to everything that served them.
        if let Some(format) = ImageFormat::from_ext(ext) {
            return format.mime();
        }
        Self(match ext.to_ascii_lowercase().as_str() {
            "html" => "text/html",
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
            "json" => "application/json",
            "webmanifest" => "application/manifest+json",
            "xml" => "application/xml",
            "txt" => "text/plain",
            "pdf" => Self::PDF,
            _ => Self::BINARY,
        })
    }

    /// The `Content-Type` header value: the type, plus a UTF-8 charset for the
    /// types that define one.
    ///
    /// Read off the media type rather than listed beside the table above, so a
    /// type added there cannot be forgotten here. It is *not* every type a
    /// human would call textual: `application/json` and the `+json` suffix
    /// register no `charset` parameter at all (RFC 8259 is explicit that adding
    /// one has no effect), while XML does (RFC 7303), which is what puts
    /// `image/svg+xml` on this side of the line and leaves
    /// `application/manifest+json` off it. The doc used to say "textual types"
    /// and the code tested `text/`, so `application/xml` and every SVG the dev
    /// server sent went out with no encoding declared.
    pub fn header(self) -> String {
        let charset =
            self.0.starts_with("text/") || self.0 == "application/xml" || self.0.ends_with("+xml");
        match charset {
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

/// A raster format the asset pipeline can decode and re-encode.
///
/// Lives here, beside [`Mime`], because classifying a file by extension is one
/// question with one answer: the optimizer used to keep its own wider list
/// (`jpe`, `jfif`) while `Mime` kept a narrower one, so the same file was
/// re-encoded as a JPEG and then served as a generic binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    /// Each format's media type and the extensions naming it. The single table
    /// both [`from_ext`](Self::from_ext) and [`Mime::of`] read, so the pipeline
    /// and everything that serves a file can never disagree about what it is.
    const FORMATS: &'static [(Self, &'static str, &'static [&'static str])] = &[
        (Self::Png, "image/png", &["png"]),
        (Self::Jpeg, "image/jpeg", &["jpg", "jpeg", "jpe", "jfif"]),
    ];

    /// The raster format a file extension names, matched case-insensitively.
    /// `None` when unrecognized. The single source for classifying a raster,
    /// independent of whether any optimization or responsive processing is
    /// enabled for it.
    pub fn from_ext(ext: &str) -> Option<Self> {
        let ext = ext.to_ascii_lowercase();
        Self::FORMATS
            .iter()
            .find(|(_, _, exts)| exts.contains(&ext.as_str()))
            .map(|(format, ..)| *format)
    }

    /// This format's media type.
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

    /// Matching ignores case. It used to not, so `Photo.PNG` was optimized as a
    /// PNG by the asset pipeline (which has always lowercased) and then served
    /// as `application/octet-stream` by everything downstream.
    #[test]
    fn extensions_match_regardless_of_case() {
        assert_eq!(Mime::of("INDEX.HTML").to_string(), "text/html");
        assert_eq!(Mime::of("Photo.PNG").to_string(), "image/png");
        assert_eq!(Mime::of("Style.CSS").to_string(), "text/css");
    }

    /// Every extension the optimizer classifies as a raster is served as one.
    /// Two tables used to answer this, and they disagreed: `.jfif` and `.jpe`
    /// were JPEG to `ImageFormat` and a generic binary to `Mime`.
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
        // A non-raster is still not one.
        assert_eq!(ImageFormat::from_ext("svg"), None);
    }

    #[test]
    fn header_adds_charset_to_the_types_that_define_one() {
        assert_eq!(Mime::of("i.html").header(), "text/html; charset=utf-8");
        assert_eq!(Mime::of("a.js").header(), "text/javascript; charset=utf-8");
        assert_eq!(Mime::of("n.txt").header(), "text/plain; charset=utf-8");
        // XML defines a charset parameter, so a suffixed type carries one too.
        assert_eq!(Mime::of("f.xml").header(), "application/xml; charset=utf-8");
        assert_eq!(Mime::of("s.svg").header(), "image/svg+xml; charset=utf-8");
        // JSON registers none, however textual it reads.
        assert_eq!(Mime::of("d.json").header(), "application/json");
        assert_eq!(
            Mime::of("m.webmanifest").header(),
            "application/manifest+json"
        );
        // A binary carries none either.
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
