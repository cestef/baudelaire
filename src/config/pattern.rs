//! A regular expression a schema holds a string to, compiled once where it is
//! written.

use regex::{Regex, RegexBuilder};

/// How large a compiled pattern may get.
///
/// A pattern is a site's own text, but a mistyped one can ask for a program far
/// larger than the line that wrote it; refusing it where it is written beats
/// carrying it into every page check.
const PROGRAM: usize = 1 << 20;

/// A compiled pattern, beside the source it was written as.
///
/// Compiled at config-parse time and carried: a schema is checked once per page
/// per field, and compiling there would be the same work over and over. The
/// source is what identity, ordering and the reference all read, since two
/// patterns are the same exactly when they were written the same.
#[derive(Debug, Clone)]
pub struct Pattern {
    source: String,
    regex: Regex,
}

impl Pattern {
    /// The pattern `source` spells, or the reason it spells none.
    ///
    /// Unanchored, as a regular expression is: `^` and `$` anchor it, and a
    /// pattern that matches anywhere is what someone writing one expects.
    pub fn parse(source: &str) -> Result<Self, regex::Error> {
        RegexBuilder::new(source)
            .size_limit(PROGRAM)
            .build()
            .map(|regex| Self {
                source: source.to_owned(),
                regex,
            })
    }

    /// Whether `text` matches.
    pub fn matches(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }

    /// The pattern as it was written.
    pub fn source(&self) -> &str {
        &self.source
    }
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.source)
    }
}

/// Two patterns are the same when they were written the same. A compiled
/// [`Regex`] has no equality of its own, and would be the wrong one anyway:
/// what a site declared is the source.
impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for Pattern {}

impl std::hash::Hash for Pattern {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.source.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::Pattern;

    #[test]
    fn a_pattern_matches_anywhere_until_it_is_anchored() {
        let bare = Pattern::parse("[a-z]+").expect("a valid pattern");
        assert!(bare.matches("abc"));
        assert!(bare.matches("1abc2"), "unanchored, as a regex is");

        let anchored = Pattern::parse("^[a-z]+$").expect("a valid pattern");
        assert!(anchored.matches("abc"));
        assert!(!anchored.matches("1abc2"));
    }

    #[test]
    fn a_pattern_that_does_not_compile_says_so() {
        assert!(Pattern::parse("[a-").is_err());
        assert!(Pattern::parse("*").is_err());
    }

    /// A pattern asking for a program larger than the ceiling is refused where
    /// it is written rather than carried into every page check.
    #[test]
    fn a_pattern_too_large_to_compile_is_refused() {
        assert!(Pattern::parse(&format!("a{{0,{}}}", 1 << 20)).is_err());
    }

    /// Identity is the source: it is what a site wrote, and what the cache
    /// fingerprint and the reference both have to agree on.
    #[test]
    fn two_patterns_are_the_same_when_they_were_written_the_same() {
        let one = Pattern::parse("^a$").expect("a valid pattern");
        let same = Pattern::parse("^a$").expect("a valid pattern");
        let other = Pattern::parse("^b$").expect("a valid pattern");

        assert_eq!(one, same);
        assert_ne!(one, other);
        assert_eq!(one.source(), "^a$");
    }
}
