//! A markdown page's frontmatter block, in whichever dialect it was written.
//!
//! Adding a dialect is one row in [`FENCES`] and one module implementing it.

mod kdl;
mod toml;
mod yaml;

use std::collections::BTreeMap;
use std::ops::Range;

use typst::foundations::Dict;

use crate::error::Result;

/// The languages a frontmatter block may be written in, each decided by its
/// fence alone, so a block is never read as a language it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Yaml,
    Toml,
    Kdl,
}

/// How deep a block may nest before it is refused, in every dialect.
///
/// Each reader converts a value by recursing, so a page's own nesting is what
/// bounds the stack: without a ceiling a block nested thousands deep aborts the
/// build with nothing said rather than failing as a page.
pub const DEPTH: usize = 64;

/// A dialect's reader, taking the text between the fences, where it sits in the
/// file, and the path and source a syntax error names and renders from.
type Read = fn(&str, usize, &str, &str) -> Result<Block>;

/// One dialect as this module meets it: the fence that opens a block, the
/// language that fence declares, and the module that reads it.
pub struct Fence {
    /// The characters that open a block in this dialect, and close it.
    pub open: &'static str,
    pub dialect: Dialect,
    read: Read,
}

/// The fence that opens and closes a block in each dialect, and the reader
/// behind it.
pub const FENCES: &[Fence] = &[
    Fence {
        open: "---",
        dialect: Dialect::Yaml,
        read: yaml::parse,
    },
    Fence {
        open: "+++",
        dialect: Dialect::Toml,
        read: toml::parse,
    },
    Fence {
        open: ";;;",
        dialect: Dialect::Kdl,
        read: kdl::parse,
    },
];

impl Dialect {
    pub fn of_fence(open: &str) -> Option<Self> {
        FENCES
            .iter()
            .find(|fence| fence.open == open)
            .map(|fence| fence.dialect)
    }

    /// Read a block written in this dialect; `offset` is where the block sits
    /// in the file, so every span it records points at the line the author
    /// wrote.
    pub fn parse(self, text: &str, offset: usize, path: &str, source: &str) -> Result<Block> {
        (self.fence().read)(text, offset, path, source)
    }

    /// The row that declares this dialect.
    fn fence(self) -> &'static Fence {
        FENCES
            .iter()
            .find(|fence| fence.dialect == self)
            .expect("every dialect has a row in FENCES")
    }
}

/// The fault a block nested past [`DEPTH`] is refused with, underlining the
/// deepest thing the author wrote where the value itself has no span.
fn too_deep(
    path: &str,
    source: &str,
    span: Option<Range<usize>>,
) -> crate::error::BaudelaireErrorKind {
    let span = span.unwrap_or(0..source.len());
    crate::error::markdown::MarkdownError::FrontmatterDepth {
        path: path.to_owned(),
        limit: DEPTH,
        src: miette::NamedSource::new(path, source.to_owned()),
        span: (span.start, span.len()).into(),
    }
    .into()
}

/// A frontmatter block, read: the fields it declares and where each was
/// written.
pub struct Block {
    pub dict: Dict,
    pub spans: Spans,
}

impl Block {
    pub fn empty() -> Self {
        Self {
            dict: Dict::new(),
            spans: Spans::default(),
        }
    }
}

/// Where each value in a frontmatter block was written, by the path of steps
/// that names it (`["author", "name"]`, `["authors", "1", "email"]`); the empty
/// path is the block itself.
///
/// The steps are kept apart rather than joined into one string, because a key
/// may itself contain the separator any joining would pick: TOML `"a.b" = 1`
/// and YAML `a.b:` are one key with a dot in it.
#[derive(Debug, Default, Clone)]
pub struct Spans(BTreeMap<Vec<String>, Range<usize>>);

impl Spans {
    /// Record where the value at `path` was written, as absolute file offsets:
    /// a dialect shifts by the block's own offset as it collects.
    pub fn insert(&mut self, path: Vec<String>, span: Range<usize>) {
        self.0.insert(path, span);
    }

    /// The path a nested key is recorded under, given its parent's.
    pub fn path(parent: &[String], key: &str) -> Vec<String> {
        let mut path = parent.to_vec();
        path.push(key.to_owned());
        path
    }

    /// The span to underline for `steps`: the deepest prefix of it the author
    /// actually wrote, so a key a page never declared stops one step short at
    /// the thing that should have held it.
    pub fn of(&self, steps: &[String]) -> Option<Range<usize>> {
        (0..=steps.len())
            .rev()
            .find_map(|depth| self.0.get(&steps[..depth]).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::{Dialect, Spans};

    #[test]
    fn each_fence_opens_one_dialect() {
        assert_eq!(Dialect::of_fence("---"), Some(Dialect::Yaml));
        assert_eq!(Dialect::of_fence("+++"), Some(Dialect::Toml));
        assert_eq!(Dialect::of_fence(";;;"), Some(Dialect::Kdl));
        assert_eq!(Dialect::of_fence("~~~"), None);
    }

    #[test]
    fn every_dialect_has_exactly_one_fence() {
        const ALL: &[Dialect] = &[Dialect::Yaml, Dialect::Toml, Dialect::Kdl];
        for dialect in ALL {
            let fences = super::FENCES
                .iter()
                .filter(|fence| fence.dialect == *dialect)
                .count();
            assert_eq!(fences, 1, "{dialect:?}");
        }
        assert_eq!(super::FENCES.len(), ALL.len(), "a fence opens one dialect");
    }

    #[test]
    fn every_fence_is_three_characters() {
        for fence in super::FENCES {
            assert_eq!(fence.open.len(), 3, "`{}`", fence.open);
        }
    }

    #[test]
    fn each_dialect_reads_the_language_its_fence_declares() {
        let blocks = [
            (Dialect::Yaml, "title: A\n"),
            (Dialect::Toml, "title = \"A\"\n"),
            (Dialect::Kdl, "title \"A\"\n"),
        ];
        for (dialect, text) in blocks {
            let block = dialect
                .parse(text, 0, "a.md", text)
                .unwrap_or_else(|_| panic!("{dialect:?} should read its own block"));
            assert_eq!(
                block.dict.at("title".into(), None),
                Ok(typst::foundations::Value::Str("A".into())),
                "{dialect:?}"
            );
        }
    }

    #[test]
    fn a_span_falls_back_to_the_deepest_thing_the_author_wrote() {
        let steps = |parts: &[&str]| parts.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

        let mut spans = Spans::default();
        spans.insert(steps(&[]), 0..100);
        spans.insert(steps(&["author"]), 10..40);
        spans.insert(steps(&["author", "name"]), 20..30);

        assert_eq!(spans.of(&steps(&["author", "name"])), Some(20..30));
        assert_eq!(spans.of(&steps(&["author", "email"])), Some(10..40));
        assert_eq!(spans.of(&steps(&["nothing", "here"])), Some(0..100));
        assert_eq!(spans.of(&[]), Some(0..100));
    }

    #[test]
    fn a_key_holding_a_dot_is_not_the_path_that_spells_it() {
        let steps = |parts: &[&str]| parts.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

        let mut spans = Spans::default();
        spans.insert(steps(&["a.b"]), 0..10);
        spans.insert(steps(&["a", "b"]), 20..30);

        assert_eq!(spans.of(&steps(&["a.b"])), Some(0..10));
        assert_eq!(spans.of(&steps(&["a", "b"])), Some(20..30));
    }
}
