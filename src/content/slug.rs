//! URL-safe slugs, the single normalization rule for every URL segment, so
//! page slugs (from a filename or frontmatter) and taxonomy terms cannot drift
//! into two different policies.

use std::fmt;

use crate::error::{ContentError, Result};

/// A URL-safe slug: lowercased letters and digits, with each run of other
/// characters collapsed to a single `-` and no leading or trailing `-`.
///
/// Letters and digits are Unicode, not ASCII. Dropping non-ASCII turned `café`
/// into `caf`, made `Café` and `Cafe` collide, and left `日本語` with no slug at
/// all: a wall for exactly the sites the i18n support exists for. Punctuation,
/// symbols and emoji are still separators, so a slug stays a single readable
/// path segment. A name with no letters or digits has no slug and the caller
/// errors. Constructed only through [`Slug::parse`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slug(String);

impl Slug {
    /// Normalize `raw` into a slug, or `None` when nothing URL-safe survives
    /// (e.g. `"!!!"` or `""`), the caller turns that into a precise error.
    pub fn parse(raw: &str) -> Option<Self> {
        use unicode_normalization::UnicodeNormalization;

        let mut out = String::with_capacity(raw.len());
        let mut pending_dash = false;
        // NFC first: a decomposed `e` + U+0301 (what macOS hands back for a
        // filename) is a letter followed by a *mark*, and a mark is not
        // alphanumeric, so it would be dropped as a separator and `café` would
        // slug to `cafe` on one machine and `café` on another.
        for c in raw.nfc() {
            if c.is_alphanumeric() {
                if pending_dash && !out.is_empty() {
                    out.push('-');
                }
                pending_dash = false;
                out.extend(c.to_lowercase());
            } else {
                pending_dash = true;
            }
        }
        (!out.is_empty()).then_some(Self(out))
    }

    /// Parse `raw`, or a precise error naming it when nothing URL-safe survives.
    /// The single "a name must yield a slug" rule, shared by pages and terms.
    pub fn require(raw: &str) -> Result<Self> {
        Self::parse(raw).ok_or_else(|| ContentError::empty_slug(raw).into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Slug;

    #[test]
    fn normalizes_to_url_safe() {
        assert_eq!(Slug::parse("Hello World").unwrap().as_str(), "hello-world");
        assert_eq!(Slug::parse("C++ & Rust").unwrap().as_str(), "c-rust");
        assert_eq!(Slug::parse("my_post").unwrap().as_str(), "my-post");
        assert_eq!(
            Slug::parse("already-clean").unwrap().as_str(),
            "already-clean"
        );
    }

    /// Letters keep their identity whatever the script; emoji and punctuation
    /// are separators like any other symbol.
    #[test]
    fn keeps_unicode_letters_and_digits() {
        assert_eq!(Slug::parse("café 🎉 page").unwrap().as_str(), "café-page");
        assert_eq!(Slug::parse("日本語").unwrap().as_str(), "日本語");
        assert_eq!(Slug::parse("Größe").unwrap().as_str(), "größe");
        assert_eq!(
            Slug::parse("Ünïcödé Post").unwrap().as_str(),
            "ünïcödé-post"
        );
    }

    /// ...so an accented letter survives instead of being dropped, which is what
    /// made `Café` and `Cafe` differ only by a truncation.
    #[test]
    fn accents_are_kept_not_dropped() {
        assert_eq!(Slug::parse("Café").unwrap().as_str(), "café");
        assert_eq!(Slug::parse("Cafe").unwrap().as_str(), "cafe");
    }

    /// The same name in either normalization form slugs identically. macOS
    /// hands back decomposed filenames, so without this a site built there and
    /// the same site built on Linux produced different URLs.
    #[test]
    fn normalization_form_does_not_change_the_slug() {
        let composed = "caf\u{e9}";
        let decomposed = "cafe\u{301}";
        assert_ne!(composed, decomposed, "the inputs must actually differ");
        assert_eq!(Slug::parse(composed), Slug::parse(decomposed));
        assert_eq!(Slug::parse(decomposed).unwrap().as_str(), "café");
    }

    #[test]
    fn rejects_empty() {
        assert!(Slug::parse("").is_none());
        assert!(Slug::parse("!!!").is_none());
        assert!(Slug::parse("  ").is_none());
        assert!(Slug::parse("🎉").is_none());
    }
}
