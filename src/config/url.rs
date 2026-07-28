//! URL shape: how a page's permalink maps onto a served path, how a
//! root-relative path becomes absolute, and how either is percent-encoded.
//! Configured through `links { style }` and `url`, but the algebra itself is
//! independent of the config tree.

use super::Named;

/// The site base URL with its trailing slash normalized away: the single
/// join rule for every consumer that makes root-relative paths absolute
/// (sitemap, feeds, robots, llms, meta tags).
#[derive(Debug, Clone)]
pub struct BaseUrl(String);

impl BaseUrl {
    /// The normalized base for a configured `url`.
    pub(super) fn new(url: &str) -> Self {
        Self(url.trim_end_matches('/').to_owned())
    }

    /// Absolute URL for a root-relative path (a permalink or `/file`),
    /// percent-encoded.
    ///
    /// Slugs keep Unicode letters, so a permalink carries raw UTF-8. Browsers
    /// cope, but an XML sitemap's `<loc>` and a feed's `<id>`/`<link>` are
    /// specified as URIs and consumers reject or mangle raw bytes there. Every
    /// absolute URL the site emits goes through here, so it is encoded once.
    pub fn join(&self, path: impl AsRef<str>) -> String {
        format!("{}{}", self.0, Percent::encode(path.as_ref()))
    }

    /// Absolute URL for a bare output file name sitting at the site root, e.g.
    /// `sitemap.xml` -> `https://site/sitemap.xml`.
    pub fn file(&self, name: &str) -> String {
        self.join(format!("/{name}"))
    }

    /// Make a root-relative `path` absolute when a base is configured, else
    /// leave it as-is: the one "absolutize if we can, otherwise stay relative"
    /// rule shared by every URL emitter. Non-root-relative refs (external URLs)
    /// pass through untouched.
    pub fn resolve(base: Option<&BaseUrl>, path: &str) -> String {
        match base {
            Some(base) if path.starts_with('/') => base.join(path),
            _ => path.to_owned(),
        }
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Percent-encoding, as a URL path carries it.
pub struct Percent;

impl Percent {
    /// Encode the bytes a URI path may not carry literally, leaving an existing
    /// `%XX` triplet alone so a path is never encoded twice.
    pub fn encode(path: &str) -> String {
        let bytes = path.as_bytes();
        let mut out = String::with_capacity(path.len());
        let mut i = 0;
        while i < bytes.len() {
            let byte = bytes[i];
            if byte == b'%' && Self::triplet(bytes, i).is_some() {
                out.push_str(&path[i..i + 3]);
                i += 3;
                continue;
            }
            match Self::literal(byte) {
                true => out.push(byte as char),
                false => out.push_str(&format!("%{byte:02X}")),
            }
            i += 1;
        }
        out
    }

    /// `%XX` triplets decoded back to bytes, everything else left alone. An
    /// invalid triplet is kept verbatim: it cannot name a real file either way,
    /// and rejecting the request would turn a typo into a 400.
    pub fn decode(path: &str) -> String {
        let bytes = path.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match Self::triplet(bytes, i).filter(|_| bytes[i] == b'%') {
                Some(byte) => {
                    out.push(byte);
                    i += 3;
                }
                None => {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
        }
        String::from_utf8(out).unwrap_or_else(|_| path.to_owned())
    }

    /// The byte a `%XX` at `i` encodes, if it is a well-formed triplet.
    fn triplet(bytes: &[u8], i: usize) -> Option<u8> {
        let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
        u8::from_str_radix(hex, 16).ok()
    }

    /// Whether a byte may appear literally in a path: RFC 3986 `unreserved`,
    /// plus the sub-delimiters and separators a site URL legitimately uses.
    fn literal(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || b"-._~/:@!$&'()*+,;=".contains(&byte)
    }
}

/// How page permalinks map onto output files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UrlStyle {
    /// Directory-per-page: `foo.typ` -> `foo/index.html`, served at `/foo/`.
    #[default]
    Clean,
    /// Flat files: `foo.typ` -> `foo.html`, served at `/foo.html`.
    Flat,
}

impl Named for UrlStyle {
    const NAMES: &'static [(&'static str, Self)] = &[("clean", Self::Clean), ("flat", Self::Flat)];
}

impl UrlStyle {
    /// Shape a page URL for this style.
    ///
    /// This is the half that used to be missing: the style only decided the
    /// output *file*, while every permalink kept the clean trailing-slash form.
    /// A flat site wrote `about.html` and then told the canonical tag, `og:url`,
    /// the sitemap, the feeds, the redirects and every rewritten `.typ` link
    /// that the page lived at `/about/`, which nothing serves. The site root is
    /// `/` under both styles.
    pub fn url(self, path: &str) -> String {
        match self {
            Self::Clean => path.to_owned(),
            Self::Flat if path == "/" || path.ends_with(Self::PAGE) => path.to_owned(),
            Self::Flat => format!("{}{}", path.trim_end_matches('/'), Self::PAGE),
        }
    }

    /// The extension a flat URL names its file with, and the single spelling of
    /// the HTML extension every path rule derives from.
    pub(crate) const PAGE: &'static str = ".html";
}
