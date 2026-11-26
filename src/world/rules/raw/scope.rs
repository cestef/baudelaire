//! The scope table: what a grammar's own names mean in baudelaire's vocabulary.

use std::str::FromStr;
use std::sync::LazyLock;

use syntect::highlighting::ScopeSelectors;
use syntect::parsing::Scope;
use typst::syntax::Tag;

use crate::config::Token;

/// A scope selector and the token it classes as, in TextMate's own selector
/// language, and the only place a scope name appears.
///
/// Matching is syntect's, so specificity settles an overlap rather than the
/// order of this table: `keyword.operator` beats `keyword`.
const SELECTORS: &[(&str, Token)] = &[
    ("comment", Token::Comment),
    ("string", Token::String),
    (
        "constant.character.escape, constant.other.escape",
        Token::Escape,
    ),
    ("constant.numeric", Token::Number),
    (
        "constant.language, constant.other, support.constant",
        Token::Constant,
    ),
    // `storage` is a keyword, not a type: a grammar scopes `let` and `struct`
    // under it, and the name they introduce under `entity.name.*`.
    ("keyword, storage", Token::Keyword),
    ("keyword.operator", Token::Operator),
    // Each exclusion leaves the punctuation to the scope that encloses it.
    (
        "punctuation - punctuation.whitespace - punctuation.definition.comment \
         - punctuation.definition.string",
        Token::Punctuation,
    ),
    (
        "entity.name.function, support.function, variable.function, meta.function-call.name",
        Token::Function,
    ),
    (
        "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, \
         support.type, support.class",
        Token::Type,
    ),
    (
        "entity.name.namespace, entity.name.module, entity.name.package",
        Token::Namespace,
    ),
    ("entity.name.tag", Token::Tag),
    ("entity.other.attribute-name", Token::Attribute),
    (
        "variable.other.member, meta.mapping.key, meta.object-literal.key, support.variable",
        Token::Property,
    ),
    ("variable", Token::Variable),
    ("variable.parameter", Token::Parameter),
    ("markup.heading, entity.name.section", Token::Heading),
    ("markup.bold", Token::Strong),
    ("markup.italic", Token::Emph),
    ("markup.underline.link, string.other.link", Token::Link),
    ("markup.raw", Token::Raw),
    ("entity.name.label, markup.other.reference", Token::Label),
    ("invalid", Token::Invalid),
];

/// [`SELECTORS`], parsed.
static MATCHERS: LazyLock<Vec<(ScopeSelectors, Token)>> = LazyLock::new(|| {
    SELECTORS
        .iter()
        .map(|(selector, token)| {
            let parsed = ScopeSelectors::from_str(selector)
                .expect("every selector in the table parses; `a_selector_is_a_scope_selector`");
            (parsed, *token)
        })
        .collect()
});

/// A grammar's scope stack, as syntect hands it over.
pub(super) struct Scopes<'a>(pub(super) &'a [Scope]);

impl Scopes<'_> {
    /// The token this stack classes as, or `None` when nothing in it is worth
    /// a class, a grammar's root and its `meta.*` groupings being structure
    /// rather than colour.
    pub(super) fn token(&self) -> Option<Token> {
        MATCHERS
            .iter()
            .filter_map(|(selector, token)| Some((selector.does_match(self.0)?, *token)))
            .max_by_key(|(power, _)| *power)
            .map(|(_, token)| token)
    }

    /// The stack as a `data-scope` stamp spells it, outermost first and
    /// space-separated, or `None` for text in the grammar's root scope alone,
    /// which is every run that is not scoped at all.
    pub(super) fn path(&self) -> Option<String> {
        (self.0.len() > 1).then(|| {
            self.0
                .iter()
                .map(|scope| scope.build_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
    }
}

/// typst's parser hands back a tag directly, mapped here into the same
/// vocabulary a sublime grammar reaches through [`SELECTORS`].
impl From<Tag> for Token {
    fn from(tag: Tag) -> Self {
        match tag {
            Tag::Comment => Self::Comment,
            Tag::Punctuation | Tag::ListMarker | Tag::MathDelimiter | Tag::MathGroupingParens => {
                Self::Punctuation
            }
            Tag::Escape => Self::Escape,
            Tag::Strong | Tag::ListTerm => Self::Strong,
            Tag::Emph => Self::Emph,
            Tag::Link => Self::Link,
            Tag::Raw => Self::Raw,
            Tag::Label | Tag::Ref => Self::Label,
            Tag::Heading => Self::Heading,
            Tag::MathOperator | Tag::Operator => Self::Operator,
            Tag::Keyword => Self::Keyword,
            Tag::Number => Self::Number,
            Tag::String => Self::String,
            Tag::Function => Self::Function,
            Tag::Interpolated => Self::Variable,
            Tag::Error => Self::Invalid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MATCHERS, SELECTORS, Scopes, Token};
    use crate::config::Named;
    use syntect::parsing::Scope;
    use typst::syntax::Tag;

    /// The token a scope stack means, by name, the stack being innermost last.
    fn token(scopes: &[&str]) -> Option<&'static str> {
        let stack: Vec<Scope> = scopes
            .iter()
            .map(|s| Scope::new(s).expect("a test scope parses"))
            .collect();
        Scopes(&stack).token().map(Token::name)
    }

    #[test]
    fn a_selector_is_a_scope_selector() {
        assert_eq!(MATCHERS.len(), SELECTORS.len());
    }

    #[test]
    fn a_scope_stack_classes_as_its_innermost_token() {
        assert_eq!(
            token(&["source.rust", "keyword.other.fn.rust"]),
            Some("keyword")
        );
        assert_eq!(
            token(&[
                "source.rust",
                "meta.function.rust",
                "entity.name.function.rust"
            ]),
            Some("function")
        );
    }

    #[test]
    fn a_longer_selector_wins_the_scope_it_shares() {
        assert_eq!(
            token(&["source.py", "keyword.operator.arithmetic.py"]),
            Some("operator")
        );
        assert_eq!(
            token(&["source.rust", "variable.parameter.rust"]),
            Some("parameter")
        );
    }

    #[test]
    fn punctuation_that_opens_something_classes_as_what_it_opens() {
        assert_eq!(
            token(&[
                "source.rust",
                "comment.line.rust",
                "punctuation.definition.comment.rust"
            ]),
            Some("comment")
        );
        assert_eq!(
            token(&[
                "source.rust",
                "string.quoted.double.rust",
                "punctuation.definition.string.begin.rust"
            ]),
            Some("string")
        );
        assert_eq!(
            token(&["source.rust", "punctuation.separator.rust"]),
            Some("punctuation")
        );
    }

    #[test]
    fn a_structural_scope_carries_no_token() {
        assert_eq!(token(&["source.rust"]), None);
        assert_eq!(token(&["source.rust", "meta.function.rust"]), None);
        assert_eq!(token(&["text.plain"]), None);
        assert_eq!(token(&["source.py", "punctuation.whitespace.py"]), None);
    }

    #[test]
    fn a_stamp_carries_the_stack_outermost_first() {
        let stack = |names: &[&str]| -> Vec<Scope> {
            names
                .iter()
                .map(|s| Scope::new(s).expect("a test scope parses"))
                .collect()
        };
        assert_eq!(
            Scopes(&stack(&["source.rust", "keyword.control.rust"])).path(),
            Some("source.rust keyword.control.rust".to_owned())
        );
        assert_eq!(Scopes(&stack(&["source.rust"])).path(), None);
    }

    #[test]
    fn a_typst_tag_lands_where_the_matching_scope_does() {
        for (tag, scope) in [
            (Tag::Keyword, "keyword.control.rust"),
            (Tag::String, "string.quoted.double.rust"),
            (Tag::Comment, "comment.line.rust"),
            (Tag::Function, "entity.name.function.rust"),
            (Tag::Number, "constant.numeric.rust"),
        ] {
            assert_eq!(
                Some(Token::from(tag).name()),
                token(&["source.rust", scope])
            );
        }
    }
}
