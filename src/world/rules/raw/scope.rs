//! The scope table: what a grammar's own names mean in baudelaire's vocabulary.

use std::str::FromStr;
use std::sync::LazyLock;

use syntect::highlighting::ScopeSelectors;
use syntect::parsing::Scope;
use typst::syntax::Tag;

use crate::config::Token;

/// A scope selector and the token it classes as, in TextMate's own selector
/// language: the grammar-to-token map, and the only place a scope name appears.
///
/// Matching is syntect's, the same it does for a theme, so specificity settles
/// the overlaps rather than the order of this table: `keyword.operator` beats
/// `keyword`, and `variable.parameter` beats `variable`. That is also why a row
/// can list alternatives and exclusions the way a `.tmTheme` would.
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
    // `storage` is a keyword, not a type: a grammar scopes `let`, `func` and
    // `struct` under it, and the name they introduce separately under
    // `entity.name.*`.
    ("keyword, storage", Token::Keyword),
    ("keyword.operator", Token::Operator),
    // Three exclusions, each because the punctuation belongs to what encloses
    // it rather than to itself: scoped indentation would wrap every leading run
    // of spaces in a span that paints nothing, and a comment's `//` or a
    // string's quote reads as part of the comment or the string, which is what
    // the enclosing scope classes them as once this row stops claiming them.
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

/// [`SELECTORS`], parsed. Compiled once for the process: a selector is a few
/// scopes and the table is fixed, so this is cheap, and doing it per code block
/// would parse the same twenty strings on every page.
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

/// A grammar's scope stack, as syntect hands it over: the token it means, and
/// the name it goes to the page under when a site asks for one.
pub(super) struct Scopes<'a>(pub(super) &'a [Scope]);

impl Scopes<'_> {
    /// The token this stack classes as, or `None` when nothing in it is worth a
    /// class: a grammar's own root (`source.rust`, `text.html`) and its `meta.*`
    /// groupings are structure, not colour, and a span around every plain word
    /// would triple the size of a code block to paint it the colour it already
    /// is.
    pub(super) fn token(&self) -> Option<Token> {
        MATCHERS
            .iter()
            .filter_map(|(selector, token)| Some((selector.does_match(self.0)?, *token)))
            .max_by_key(|(power, _)| *power)
            .map(|(_, token)| token)
    }

    /// The most specific scope in the stack, which is the one a `data-scope`
    /// stamp names. Empty for a stack with nothing in it, which no classed piece
    /// has.
    pub(super) fn name(&self) -> String {
        self.0
            .last()
            .map(|scope| scope.build_string())
            .unwrap_or_default()
    }
}

/// typst's own code needs no scope round-trip: its parser hands back the tag
/// directly, and this maps it into the same vocabulary a sublime grammar
/// reaches through [`SELECTORS`]. A `#let` in a `typ` block and a `let` in a
/// `rust` one are both `sx-keyword`.
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

    /// The token a scope stack means, by name. The stack is innermost last, the
    /// way a grammar hands it over.
    fn token(scopes: &[&str]) -> Option<&'static str> {
        let stack: Vec<Scope> = scopes
            .iter()
            .map(|s| Scope::new(s).expect("a test scope parses"))
            .collect();
        Scopes(&stack).token().map(Token::name)
    }

    /// The table is a `const` of hand-written selectors, and nothing else parses
    /// them: a typo would otherwise surface as a panic on the first code block.
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

    /// Specificity settles an overlap, not the order of the table: both
    /// `keyword` and `keyword.operator` match here, and the longer selector is
    /// the one that means something.
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

    /// A comment's `//` and a string's quote are scoped as punctuation *of* the
    /// thing they open. Classing them as punctuation paints two characters a
    /// different colour from the twenty they introduce, so the row that would
    /// claim them stands aside and the enclosing scope answers instead.
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
        // Punctuation that opens nothing is still punctuation.
        assert_eq!(
            token(&["source.rust", "punctuation.separator.rust"]),
            Some("punctuation")
        );
    }

    /// Structure carries no class at all.
    #[test]
    fn a_structural_scope_carries_no_token() {
        assert_eq!(token(&["source.rust"]), None);
        assert_eq!(token(&["source.rust", "meta.function.rust"]), None);
        assert_eq!(token(&["text.plain"]), None);
        assert_eq!(token(&["source.py", "punctuation.whitespace.py"]), None);
    }

    /// A `data-scope` stamp names the most specific scope, which is the one a
    /// stylesheet reaching past the vocabulary wants to select on.
    #[test]
    fn a_stamp_names_the_most_specific_scope() {
        let stack: Vec<Scope> = ["source.rust", "keyword.control.rust"]
            .iter()
            .map(|s| Scope::new(s).expect("a test scope parses"))
            .collect();
        assert_eq!(Scopes(&stack).name(), "keyword.control.rust");
    }

    /// A typst tag and a grammar's scope have to land in the same token, or a
    /// stylesheet would need one palette per source of highlighting.
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
