//! A markdown page's frontmatter block, in whichever dialect it was written.
//!
//! Three dialects reach one [`Block`], and nothing downstream knows which one it
//! came from: the dict goes through the same
//! [`FIELDS`](crate::content::frontmatter) walk a typst page's export does, so
//! built-in keys, configured taxonomy keys, the typo suggester and the
//! collection schema are all the ones already there.
//!
//! Adding a dialect is one row in [`FENCES`], one arm in [`Dialect::parse`],
//! and one module implementing it. Nothing else in the codebase learns the
//! fence: the splitter and the diagnostics both read the table.
//!
//! # Spans
//!
//! Every dialect resolves its own document *once*, into a [`Spans`] map keyed by
//! the path of steps that names each value. Diagnostics then walk one structure
//! rather than three, and a parser's document model does not have to outlive the
//! parse. That last part is not a nicety: `saphyr` borrows the input it parsed
//! and `toml_edit` owns it, so keeping the three document types alive together
//! would mean a lifetime for each.

mod kdl;
mod toml;
mod yaml;

use std::collections::BTreeMap;
use std::ops::Range;

use typst::foundations::Dict;

use crate::error::Result;

/// The languages a frontmatter block may be written in.
///
/// The fence says which, so a block is never read as a language it is not: the
/// alternative was one fence carrying a name after it, and a `---` block whose
/// name was misspelled would then have parsed as the default and reported the
/// confusion as a dozen unknown keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Yaml,
    Toml,
    Kdl,
}

/// The fence that opens and closes a block in each dialect.
///
/// Two of the three are the conventions already in the world: `---` is YAML,
/// which is what every generator but one uses and what a pasted post already
/// carries, and `+++` is TOML, which is Zola's and Hugo's. `;;;` is KDL, whose
/// own statement terminator is the semicolon, and which is the one spelling
/// CommonMark gives no meaning to at the start of a line.
///
/// The fence *is* the selector: there is no name after it to get wrong, and no
/// dialect can be reached by two spellings.
pub const FENCES: &[(&str, Dialect)] = &[
    ("---", Dialect::Yaml),
    ("+++", Dialect::Toml),
    (";;;", Dialect::Kdl),
];

impl Dialect {
    /// The dialect a fence opens.
    pub fn of_fence(fence: &str) -> Option<Self> {
        FENCES
            .iter()
            .find(|(open, _)| *open == fence)
            .map(|(_, dialect)| *dialect)
    }

    /// Read a block written in this dialect.
    ///
    /// `offset` is where the block sits in the file, so every span this records
    /// already points at the line the author wrote rather than at the block's
    /// own first line. `path` and `source` are the file a syntax error names and
    /// renders its snippet from.
    pub fn parse(self, text: &str, offset: usize, path: &str, source: &str) -> Result<Block> {
        match self {
            Self::Yaml => yaml::parse(text, offset, path, source),
            Self::Toml => toml::parse(text, offset, path, source),
            Self::Kdl => kdl::parse(text, offset, path, source),
        }
    }
}

/// A frontmatter block, read: the fields it declares and where each was written.
pub struct Block {
    /// The fields, as the dict every reader downstream already takes.
    pub dict: Dict,
    /// Where each of them sits in the file.
    pub spans: Spans,
}

impl Block {
    /// The block a page with no frontmatter at all declares. Empty, and still
    /// walked: a collection schema may require a field the page never wrote, and
    /// that is exactly what the walk exists to catch.
    pub fn empty() -> Self {
        Self {
            dict: Dict::new(),
            spans: Spans::default(),
        }
    }
}

/// Where each value in a frontmatter block was written, by the path of steps
/// that names it (`["author", "name"]`, `["authors", "1", "email"]`).
///
/// One representation for every dialect, and the reason diagnostics do not carry
/// a parser's document model around. The empty path is the block itself, which is
/// where a reader goes to add a field that is missing.
///
/// The steps are kept apart rather than joined into one string, because a key
/// may itself contain the separator any joining would pick: TOML `"a.b" = 1` and
/// YAML `a.b:` are one key with a dot in it, and joined with `.` they collided
/// with the path of `a` → `b` and underlined each other's value.
#[derive(Debug, Default, Clone)]
pub struct Spans(BTreeMap<Vec<String>, Range<usize>>);

impl Spans {
    /// Record where the value at `path` was written. Absolute file offsets: a
    /// dialect shifts by the block's own offset as it collects, so nothing
    /// downstream has to remember to.
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
    /// actually wrote.
    ///
    /// A nested key a page never declared stops one step short, at the thing
    /// that should have held it, which is where a reader would go to add it.
    /// The same rule the typst walk follows, so a diagnostic reads the same
    /// whichever dialect produced it.
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

    /// Every dialect is reachable by exactly one fence: one that shipped
    /// without a fence would be unwritable, and one reachable by two would make
    /// the fence stop being the selector.
    ///
    /// `ALL` is the only place a variant is listed twice, and the length check
    /// below is what keeps it honest: a fourth dialect added to [`FENCES`] and
    /// not here fails on the count, and one added here and not to `FENCES` fails
    /// on its own row.
    #[test]
    fn every_dialect_has_exactly_one_fence() {
        const ALL: &[Dialect] = &[Dialect::Yaml, Dialect::Toml, Dialect::Kdl];
        for dialect in ALL {
            let fences = super::FENCES.iter().filter(|(_, d)| d == dialect).count();
            assert_eq!(fences, 1, "{dialect:?}");
        }
        assert_eq!(super::FENCES.len(), ALL.len(), "a fence opens one dialect");
    }

    /// Every fence is the same width, which is what lets the splitter read one
    /// before it knows which it found.
    #[test]
    fn every_fence_is_three_characters() {
        for (fence, _) in super::FENCES {
            assert_eq!(fence.len(), 3, "`{fence}`");
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
        // Never written, so the dict that should have held it is underlined.
        assert_eq!(spans.of(&steps(&["author", "email"])), Some(10..40));
        assert_eq!(spans.of(&steps(&["nothing", "here"])), Some(0..100));
        assert_eq!(spans.of(&[]), Some(0..100));
    }

    /// A key with a separator in it is not a path. Joined into one string,
    /// `"a.b"` and `a` → `b` were the same key, and each underlined the other's
    /// value.
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
