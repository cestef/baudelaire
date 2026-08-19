//! The parsers a fence is checked with in-process, one per language family.
//!
//! [`builtin`] is the single source of which languages a `lint { snippets { } }`
//! line may name without giving a command of its own.

use std::sync::LazyLock;

use typst::syntax::{DiagSpanKind, Source};

use crate::render::snippet::{Fault, Position, Snippet};
use crate::world::rules::TYPST;

/// The registered parsers, looked up by the language a fence names.
pub struct Parsers;

impl Parsers {
    /// The parser claiming `lang`, or `None` for a language nothing parses.
    pub fn of(lang: &str) -> Option<&'static dyn Parser> {
        PARSERS
            .iter()
            .find(|parser| parser.langs().contains(&lang))
            .map(AsRef::as_ref)
    }

    /// Every language a fence may name without giving a command of its own, in
    /// registration order.
    pub fn langs() -> Vec<&'static str> {
        PARSERS
            .iter()
            .flat_map(|parser| parser.langs())
            .copied()
            .collect()
    }
}

/// A language whose snippets baudelaire can check without spawning anything.
pub trait Parser: Send + Sync {
    /// The fence languages this claims, in the spelling an author writes after
    /// the backticks.
    fn langs(&self) -> &'static [&'static str];

    /// What is wrong with `snippet`; empty when nothing is.
    fn check(&self, snippet: &Snippet) -> Vec<Fault>;
}

static PARSERS: LazyLock<Vec<Box<dyn Parser>>> = LazyLock::new(builtin);

/// The built-in parsers. A new language is one `impl` plus one line here, and
/// `lint { snippets { } }` accepts its name the same moment.
fn builtin() -> Vec<Box<dyn Parser>> {
    vec![
        Box::new(Kdl),
        Box::new(Json),
        Box::new(Toml),
        Box::new(Yaml),
        Box::new(Typ),
    ]
}

struct Kdl;

impl Parser for Kdl {
    fn langs(&self) -> &'static [&'static str] {
        &["kdl"]
    }

    fn check(&self, snippet: &Snippet) -> Vec<Fault> {
        match snippet.text().parse::<kdl::KdlDocument>() {
            Ok(_) => Vec::new(),
            Err(error) => error
                .diagnostics
                .iter()
                .map(|d| Fault {
                    message: d.message.clone().unwrap_or_else(|| d.to_string()),
                    at: Some(d.span.offset()),
                })
                .collect(),
        }
    }
}

struct Json;

impl Parser for Json {
    fn langs(&self) -> &'static [&'static str] {
        &["json"]
    }

    fn check(&self, snippet: &Snippet) -> Vec<Fault> {
        match serde_json::from_str::<serde_json::Value>(snippet.text()) {
            Ok(_) => Vec::new(),
            Err(error) => vec![Fault {
                message: error.to_string(),
                at: snippet.offset(Position {
                    line: error.line(),
                    column: error.column(),
                }),
            }],
        }
    }
}

struct Toml;

impl Parser for Toml {
    fn langs(&self) -> &'static [&'static str] {
        &["toml"]
    }

    fn check(&self, snippet: &Snippet) -> Vec<Fault> {
        match toml_edit::ImDocument::parse(snippet.text()) {
            Ok(_) => Vec::new(),
            Err(error) => vec![Fault {
                message: error.message().to_owned(),
                at: error.span().map(|span| span.start),
            }],
        }
    }
}

struct Yaml;

impl Parser for Yaml {
    fn langs(&self) -> &'static [&'static str] {
        &["yaml", "yml"]
    }

    /// `saphyr`'s marker index counts characters whatever its accessor is
    /// called, so it goes through `char_indices` rather than straight into a
    /// byte offset.
    fn check(&self, snippet: &Snippet) -> Vec<Fault> {
        use saphyr::LoadableYamlNode as _;

        match saphyr::MarkedYaml::load_from_str(snippet.text()) {
            Ok(_) => Vec::new(),
            Err(error) => vec![Fault {
                message: error.info().to_owned(),
                at: snippet
                    .text()
                    .char_indices()
                    .nth(error.marker().index())
                    .map(|(at, _)| at),
            }],
        }
    }
}

/// Typst, parsed but not evaluated: a fence is a sample, and the names it
/// reaches for live in the page it was lifted from rather than in itself.
struct Typ;

impl Parser for Typ {
    fn langs(&self) -> &'static [&'static str] {
        TYPST
    }

    fn check(&self, snippet: &Snippet) -> Vec<Fault> {
        let source = Source::detached(snippet.text());
        let (errors, _) = source.root().errors_and_warnings();
        errors
            .into_iter()
            .map(|error| Fault {
                message: error.message.to_string(),
                at: match error.span.get() {
                    DiagSpanKind::Number { num, sub_range, .. } => {
                        source.range(num, sub_range).map(|range| range.start)
                    }
                    DiagSpanKind::Range { range, .. } => Some(range.start),
                    DiagSpanKind::Detached => None,
                },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use typst::syntax::Span;

    use super::{Parsers, Snippet};

    fn snippet(lang: &str, text: &str) -> Snippet {
        Snippet::new(lang, text, Span::detached())
    }

    #[test]
    fn a_well_formed_snippet_has_nothing_to_report() {
        for (lang, text) in [
            ("kdl", "site \"x\"\n"),
            ("json", "{\"a\": 1}"),
            ("typ", "#let x = 1\n"),
            ("toml", "a = 1\n"),
            ("yaml", "a: 1\n"),
        ] {
            let parser = Parsers::of(lang).unwrap_or_else(|| panic!("{lang} has a parser"));
            let faults = parser.check(&snippet(lang, text));
            assert!(faults.is_empty(), "{lang} objected to {text:?}");
        }
    }

    #[test]
    fn a_malformed_snippet_is_reported_where_it_broke() {
        for (lang, text) in [
            ("kdl", "site \"x\n"),
            ("json", "{\"a\": }"),
            ("typst", "#let x = [\n"),
            ("toml", "a = \n"),
            ("yaml", "a: [1\n"),
        ] {
            let parser = Parsers::of(lang).unwrap_or_else(|| panic!("{lang} has a parser"));
            let faults = parser.check(&snippet(lang, text));
            assert!(!faults.is_empty(), "{lang} accepted {text:?}");
            for fault in faults {
                assert!(!fault.message.trim().is_empty(), "{lang} said nothing");
                if let Some(at) = fault.at {
                    assert!(at <= text.len(), "{lang} pointed past the snippet");
                }
            }
        }
    }

    #[test]
    fn a_language_nothing_parses_has_no_parser() {
        assert!(Parsers::of("sh").is_none());
        assert!(Parsers::langs().contains(&"kdl"));
    }
}
