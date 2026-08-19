//! The grammars baudelaire carries for languages syntect's bundled set has
//! none of, starting with the one its own config is written in.

use std::sync::{Arc, LazyLock};

use syntect::parsing::{SyntaxDefinition, SyntaxSet, SyntaxSetBuilder};

/// A language and the grammar that highlights it. Adding one is one
/// `include_str!` plus one line, and every surface that highlights -- the built
/// pages, `config show`, a `--help` example -- picks it up at once.
const SHIPPED: &[&str] = &[include_str!("kdl.sublime-syntax")];

/// The loaded grammars. One that fails to parse is dropped rather than
/// panicking, because the test below already refuses a build that ships one.
static SET: LazyLock<Arc<SyntaxSet>> = LazyLock::new(|| {
    let mut builder = SyntaxSetBuilder::new();
    for definition in SHIPPED
        .iter()
        .filter_map(|grammar| SyntaxDefinition::load_from_str(grammar, false, None).ok())
    {
        builder.add(definition);
    }
    Arc::new(builder.build())
});

/// The grammars that ship with the binary.
pub(super) struct Shipped;

impl Shipped {
    /// The shipped set, for a language the bundled one does not name.
    pub(super) fn set() -> Arc<SyntaxSet> {
        SET.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{SHIPPED, Shipped};

    #[test]
    fn every_shipped_grammar_loads() {
        assert_eq!(
            Shipped::set().syntaxes().len(),
            SHIPPED.len(),
            "a shipped grammar failed to parse"
        );
    }

    #[test]
    fn the_config_language_is_one_of_them() {
        assert!(Shipped::set().find_syntax_by_token("kdl").is_some());
    }
}
