//! URL-safe slugs — the single normalization rule for every URL segment, so
//! page slugs (from a filename or frontmatter) and taxonomy terms cannot drift
//! into two different policies.

use std::fmt;

use crate::error::{ContentError, Result};

/// A URL-safe slug: lowercase ASCII, with each run of other characters
/// collapsed to a single `-` and no leading or trailing `-`. ASCII-only keeps
/// every emitted URL clean without per-emitter percent-encoding; a name with no
/// ASCII letters/digits has no slug (the caller errors). Constructed only
/// through [`Slug::parse`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slug(String);

impl Slug {
    /// Normalize `raw` into a slug, or `None` when nothing URL-safe survives
    /// (e.g. `"!!!"` or `""`) — the caller turns that into a precise error.
    pub fn parse(raw: &str) -> Option<Self> {
        let mut out = String::with_capacity(raw.len());
        let mut pending_dash = false;
        for c in raw.chars() {
            if c.is_ascii_alphanumeric() {
                if pending_dash && !out.is_empty() {
                    out.push('-');
                }
                pending_dash = false;
                out.push(c.to_ascii_lowercase());
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
        // Non-ASCII is dropped to keep URLs clean without percent-encoding.
        assert_eq!(Slug::parse("café 🎉 page").unwrap().as_str(), "caf-page");
    }

    #[test]
    fn rejects_empty() {
        assert!(Slug::parse("").is_none());
        assert!(Slug::parse("!!!").is_none());
        assert!(Slug::parse("  ").is_none());
        assert!(Slug::parse("日本語").is_none());
    }
}
