//! Table-driven config dispatch.
//!
//! [`Block`] matches a scope's child nodes by name; [`Attrs`] matches a node's
//! `key=value` entries. Each dispatch table is the *single source of truth* for
//! that scope's valid keys: [`Keys`] derives "unknown key" errors (with a
//! nearest-match hint) from the very same table, so suggestions never drift from
//! what actually parses.

use kdl::{KdlNode, KdlValue};
use miette::SourceSpan;

use crate::error::{BaudelaireErrorKind, ConfigError, Result};

use super::parse::{EntryExt, NodeExt};

/// A `(key, handler)` rule for a node-keyed [`Block`] scope.
type Rule<T> = (&'static str, fn(&mut T, &KdlNode, &str) -> Result<()>);

/// A `(key, handler)` rule for an attribute-keyed [`Attrs`] scope.
type Attr<T> = (
    &'static str,
    fn(&mut T, &KdlValue, &str, SourceSpan) -> Result<()>,
);

/// A node-keyed scope (child nodes matched by name), e.g. the top-level config
/// or a `serve { ... }` block. The rule table is the single source of truth for
/// valid keys.
pub(super) struct Block<T: 'static>(pub(super) &'static [Rule<T>]);

impl<T> Block<T> {
    /// Apply this scope's rules to every node in `nodes`, erroring on the first
    /// unrecognized key (with a nearest-match suggestion).
    pub(super) fn apply(&self, value: &mut T, nodes: &[KdlNode], text: &str) -> Result<()> {
        for node in nodes {
            let key = node.name().value();
            match self.0.iter().find(|(k, _)| *k == key) {
                Some((_, handler)) => handler(value, node, text)?,
                None => return Err(Keys::unknown_key(self.0, text, key, NodeExt::span(node))),
            }
        }
        Ok(())
    }

    /// Apply this scope's rules to a node's `{ ... }` children block.
    pub(super) fn fill(&self, value: &mut T, node: &KdlNode, text: &str) -> Result<()> {
        self.apply(value, node.block(text)?.nodes(), text)
    }
}

/// An attribute-keyed scope (a node's `key=value` entries), e.g. a single
/// `content { collections { posts sort=... } }` line. Same single-source-of-truth
/// contract
/// as [`Block`], but handlers receive the attribute value.
pub(super) struct Attrs<T: 'static>(pub(super) &'static [Attr<T>]);

impl<T> Attrs<T> {
    /// Apply named attributes of `node`, erroring on the first unrecognized
    /// attribute. At most `leading` positional (unnamed) entries are tolerated,
    /// and only at the front of the node (the caller consumes them, e.g. a
    /// collection's glob): any other positional would be silently discarded,
    /// so it errors instead.
    pub(super) fn apply(
        &self,
        value: &mut T,
        node: &KdlNode,
        text: &str,
        leading: usize,
    ) -> Result<()> {
        let span = NodeExt::span(node);
        for (position, entry) in node.entries().iter().enumerate() {
            let Some(key) = entry.name().map(|n| n.value()) else {
                if position >= leading {
                    return Err(ConfigError::unexpected_argument(
                        text,
                        &entry.value().to_string(),
                        node.name().value(),
                        EntryExt::span(entry),
                    )
                    .into());
                }
                continue;
            };
            match self.0.iter().find(|(k, _)| *k == key) {
                Some((_, handler)) => handler(value, entry.value(), text, span)?,
                None => return Err(Keys::unknown_key(self.0, text, key, span)),
            }
        }
        Ok(())
    }
}

/// The valid keys of a scope, derived from its dispatch table (never a separate
/// hand-kept list). Builds "unknown key" errors carrying a nearest-match hint.
pub(crate) struct Keys<'a>(pub(super) &'a [&'a str]);

impl<'a> Keys<'a> {
    /// The single "closest known name" helper, reused wherever a typo should
    /// suggest a valid name (config keys, frontmatter fields).
    pub(crate) fn of(names: &'a [&'a str]) -> Self {
        Self(names)
    }
}

impl Keys<'_> {
    /// Build an unknown-*key* error (a structural node/attribute name) from any
    /// dispatch `table`. The table is the sole source of truth for validity, so
    /// suggestions can never drift from what actually parses.
    pub(super) fn unknown_key<F>(
        table: &[(&'static str, F)],
        text: &str,
        key: &str,
        span: SourceSpan,
    ) -> BaudelaireErrorKind {
        let names: Vec<&str> = table.iter().map(|(k, _)| *k).collect();
        ConfigError::unknown_key(text, key, Keys(&names).help(key, "keys"), span).into()
    }

    /// Build an unknown-*value* error (an unrecognized enum variant supplied as
    /// a value) from an allowed-values `table`: the value counterpart to
    /// [`Keys::unknown_key`].
    pub(super) fn unknown_value<F>(
        table: &[(&'static str, F)],
        text: &str,
        value: &str,
        span: SourceSpan,
    ) -> BaudelaireErrorKind {
        let names: Vec<&str> = table.iter().map(|(k, _)| *k).collect();
        ConfigError::unknown_value(text, value, Keys(&names).help(value, "values"), span).into()
    }

    /// "did you mean ..? valid `noun`: .." help for an unrecognized name, reused
    /// wherever a name set drives validity (dispatch keys, profile names,
    /// virtual Typst modules).
    pub(crate) fn help(&self, unknown: &str, noun: &str) -> String {
        let mut help = match self.nearest(unknown) {
            Some(near) => format!("did you mean `{near}`? "),
            None => String::new(),
        };
        help.push_str(&format!("valid {noun}: "));
        help.push_str(&self.0.join(", "));
        help
    }

    /// The valid key within edit distance 2 of `unknown` (a typo), if any.
    pub(crate) fn nearest(&self, unknown: &str) -> Option<&str> {
        self.0
            .iter()
            .copied()
            .map(|candidate| (candidate, Self::distance(candidate, unknown)))
            .filter(|&(_, d)| d <= 2)
            .min_by_key(|&(_, d)| d)
            .map(|(candidate, _)| candidate)
    }

    /// Levenshtein edit distance between two words.
    fn distance(a: &str, b: &str) -> usize {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        let mut curr = vec![0; b.len() + 1];
        for (i, &ca) in a.iter().enumerate() {
            curr[0] = i + 1;
            for (j, &cb) in b.iter().enumerate() {
                let cost = usize::from(ca != cb);
                curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }
        prev[b.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::Keys;

    #[test]
    fn suggests_the_nearest_key_for_a_typo() {
        assert_eq!(
            Keys(&["content", "dist"]).nearest("conten"),
            Some("content")
        );
        assert_eq!(Keys(&["port", "bind"]).nearest("prt"), Some("port"));
    }

    #[test]
    fn offers_no_suggestion_for_unrelated_words() {
        assert_eq!(Keys(&["content", "dist"]).nearest("xyzzy"), None);
    }

    #[test]
    fn help_lists_valid_keys_and_suggestion() {
        let help = Keys(&["pretty"]).help("pruty", "keys");
        assert!(help.contains("did you mean `pretty`?"), "{help}");
        assert!(help.contains("valid keys: pretty"), "{help}");
    }
}
