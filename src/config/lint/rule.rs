//! The lint rules by name: the vocabulary the config keys, the exemption marker
//! and a finding's severity lookup all read from.

use crate::config::Named;

/// One rule a finding can come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rule {
    Headings,
    Alt,
    Ids,
    Aria,
    Snippets,
}

impl Rule {
    /// This rule's name, in a `const` context: the key its config row is
    /// declared under, so the table and the vocabulary cannot drift.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Headings => "headings",
            Self::Alt => "alt",
            Self::Ids => "ids",
            Self::Aria => "aria",
            Self::Snippets => "snippets",
        }
    }
}

impl Named for Rule {
    const NAMES: &'static [(&'static str, Self)] = &[
        (Self::Headings.key(), Self::Headings),
        (Self::Alt.key(), Self::Alt),
        (Self::Ids.key(), Self::Ids),
        (Self::Aria.key(), Self::Aria),
        (Self::Snippets.key(), Self::Snippets),
    ];
}

/// Which rule a finding came from, and for a snippet the language whose line
/// carries its loudness.
///
/// A finding is cached with its page and its severity is not, so resolving the
/// severity from the rule at report time is what keeps a cache hit reporting
/// what the current config asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ruled {
    pub rule: Rule,
    /// The fence language, for [`Rule::Snippets`] only.
    pub lang: Option<String>,
}

impl From<Rule> for Ruled {
    fn from(rule: Rule) -> Self {
        Self { rule, lang: None }
    }
}

impl Ruled {
    /// A snippet finding, which takes its loudness from the language's own line.
    pub fn snippet(lang: &str) -> Self {
        Self {
            rule: Rule::Snippets,
            lang: Some(lang.to_owned()),
        }
    }
}
