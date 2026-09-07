//! URL shape: how a page's permalink maps onto a served path, how a
//! root-relative path becomes absolute, and how either is percent-encoded.

use std::fmt::Write as _;
use std::path::Path;

use super::Named;

/// The site base URL with its trailing slash normalized away: the single join
/// rule for every consumer that makes root-relative paths absolute.
#[derive(Debug, Clone)]
pub struct BaseUrl(String);

/// A URL scheme, as RFC 3986 spells one.
pub struct Scheme;

impl Scheme {
    /// Whether `text` is a scheme: a letter, then letters, digits, `+`, `-` or
    /// `.`. Stated once, because a config that accepts a base URL and a render
    /// pass that classifies a link as external have to agree on what a scheme
    /// is or one publishes what the other refused.
    pub fn valid(text: &str) -> bool {
        let mut chars = text.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    }
}

impl BaseUrl {
    pub(super) fn new(url: &str) -> Self {
        Self(url.trim_end_matches('/').to_owned())
    }

    /// Whether `url` is an absolute base: a scheme, `://`, and a non-empty
    /// host, which everything downstream assumes.
    ///
    /// Not [`NodeExt::url`](super::node::NodeExt::url)'s check: that one demands
    /// https because credentials travel to those hosts, while a site is served
    /// to readers and `http://` is theirs to choose.
    pub fn absolute(url: &str) -> bool {
        let Some((scheme, rest)) = url.split_once("://") else {
            return false;
        };
        let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
        Scheme::valid(scheme) && !host.is_empty() && !host.contains(char::is_whitespace)
    }

    /// Absolute URL for a root-relative path (a permalink or `/file`),
    /// percent-encoded.
    ///
    /// Slugs keep Unicode letters, so a permalink carries raw UTF-8, which a
    /// sitemap `<loc>` and a feed `<id>` may not: every absolute URL the site
    /// emits goes through here, so it is encoded once.
    pub fn join(&self, path: impl AsRef<str>) -> String {
        format!("{}{}", self.0, Percent::encode(path.as_ref()))
    }

    /// Absolute URL for a bare output file name sitting at the site root, e.g.
    /// `sitemap.xml` -> `https://site/sitemap.xml`.
    pub fn file(&self, name: &str) -> String {
        self.join(format!("/{name}"))
    }

    /// Make a root-relative `path` absolute when a base is configured, else
    /// leave it as-is. External URLs pass through untouched.
    pub fn resolve(base: Option<&Self>, path: &str) -> String {
        match base {
            Some(base) if path.starts_with('/') => base.join(path),
            _ => path.to_owned(),
        }
    }

    /// The path component of a configured `url`, trailing slash normalized away
    /// (`https://host/docs/` -> `/docs`); empty for a root-hosted site.
    pub fn path(url: &str) -> &str {
        let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
        rest.find('/')
            .map_or("", |slash| rest[slash..].trim_end_matches('/'))
    }

    /// The scheme and host, without the site's own path component.
    ///
    /// What absolutizes a URL that already carries the base path, as captured
    /// page markup does and a permalink does not.
    pub fn origin(&self) -> &str {
        let path = Self::path(&self.0);
        if path.is_empty() {
            &self.0
        } else {
            self.0.strip_suffix(path).unwrap_or(&self.0)
        }
    }

    /// [`resolve`](Self::resolve) against the origin alone, for markup whose
    /// paths the base-path transform has already prefixed.
    pub fn rebase(base: Option<&Self>, path: &str) -> String {
        match base {
            Some(base) if path.starts_with('/') => {
                format!("{}{}", base.origin(), Percent::encode(path))
            }
            _ => path.to_owned(),
        }
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The file name a permalink's paged artifacts hang off: `/posts/a/` is
/// `posts/a`, `/about.html` is `about`, and the home page is `index`.
///
/// Not [`crate::content::Stem`], which is what a *source* filename says about a
/// page.
pub struct Basename<'a>(pub &'a str);

impl std::fmt::Display for Basename<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stem = self.0.trim_matches('/');
        let stem = stem.strip_suffix(".html").unwrap_or(stem);
        f.write_str(if stem.is_empty() { "index" } else { stem })
    }
}

/// A relative path spelled as the URL it is served at: separators become `/`
/// whatever the host filesystem writes.
///
/// The one rule, shared by everything that turns a file's place in a tree into
/// a name a request can carry. The producer at
/// [`Config::asset_url`](super::Config::asset_url) and the consumer matching
/// against it have to agree, so neither spells it itself.
pub struct Slashed<'a>(pub &'a Path);

impl std::fmt::Display for Slashed<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, component) in self.0.to_string_lossy().split('\\').enumerate() {
            if index > 0 {
                f.write_char('/')?;
            }
            f.write_str(component)?;
        }
        Ok(())
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
            if Self::literal(byte) {
                out.push(byte as char);
            } else {
                let _ = write!(out, "%{byte:02X}");
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
            if let Some(byte) = Self::triplet(bytes, i).filter(|_| bytes[i] == b'%') {
                out.push(byte);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
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
    /// Shape a page URL for this style. The site root is `/` under both.
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

#[cfg(test)]
mod tests {
    use super::BaseUrl;

    #[test]
    fn a_base_splits_into_an_origin_and_a_path() {
        for (url, origin, path) in [
            ("https://host.test/docs", "https://host.test", "/docs"),
            ("https://host.test/docs/", "https://host.test", "/docs"),
            ("https://host.test", "https://host.test", ""),
            ("https://host.test/", "https://host.test", ""),
            ("https://host.test/a/b", "https://host.test", "/a/b"),
        ] {
            assert_eq!(BaseUrl::path(url), path, "path of {url}");
            assert_eq!(BaseUrl::new(url).origin(), origin, "origin of {url}");
        }
    }

    /// A permalink carries no base path, so it joins the whole base; captured
    /// markup already carries one, so it joins the origin alone.
    #[test]
    fn a_prefixed_path_is_absolutised_against_the_origin_alone() {
        let base = BaseUrl::new("https://host.test/docs");
        assert_eq!(
            BaseUrl::resolve(Some(&base), "/posts/b/"),
            "https://host.test/docs/posts/b/"
        );
        assert_eq!(
            BaseUrl::rebase(Some(&base), "/docs/posts/b/"),
            "https://host.test/docs/posts/b/"
        );
        let root = BaseUrl::new("https://host.test");
        assert_eq!(
            BaseUrl::rebase(Some(&root), "/posts/b/"),
            BaseUrl::resolve(Some(&root), "/posts/b/")
        );
        assert_eq!(
            BaseUrl::rebase(Some(&base), "https://elsewhere.test/x"),
            "https://elsewhere.test/x"
        );
    }
}
