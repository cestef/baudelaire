//! The record contract: what `git` is asked to print about a commit, and how
//! what it printed is read back.
//!
//! One [`Format`] states both, so a field added to the request is a field the
//! reader has to destructure.

use std::fmt::{self, Write as _};

use super::path::Quoted;

/// The byte a record opens with, so a commit can be told from the files listed
/// under it.
///
/// A path line can never begin with it: git C-quotes any path holding a control
/// character, whatever `core.quotepath` says, so a quoted path begins with `"`
/// and a bare one with a printable byte.
const RECORD: char = '\u{1}';

/// The byte between a record's fields.
///
/// A control character, because git renders each field verbatim and has no way
/// to escape one that holds the separator; only a byte that cannot occur in a
/// hash or a date will do.
const FIELD: char = '\u{1f}';

/// A placeholder git substitutes when it prints a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    /// The full commit SHA.
    Hash,
    /// The committer date, ISO-8601.
    Committed,
    /// The author's name, through `.mailmap` where the repository has one.
    Name,
    /// The author's email, likewise. Free text, so it goes last: git forbids a
    /// newline in an ident but not the field separator.
    Email,
}

impl Field {
    const fn placeholder(self) -> &'static str {
        match self {
            Self::Hash => "%H",
            Self::Committed => "%cI",
            Self::Name => "%an",
            Self::Email => "%aE",
        }
    }
}

/// The fields one record carries, in the order git is asked to print them.
///
/// Free text goes last. Git cannot escape a field holding the separator, and
/// reading with [`str::splitn`] keeps a stray one inside the final field rather
/// than shifting every field after it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Format<const N: usize>([Field; N]);

impl<const N: usize> Format<N> {
    pub(super) const fn new(fields: [Field; N]) -> Self {
        Self(fields)
    }

    /// The fields of one record.
    ///
    /// `None` where git wrote fewer than the format asked for, which is a git
    /// that did not understand it. Which fields those are does not come into
    /// it: how many there are is the whole contract, and the format fixes it.
    fn read(record: &str) -> Option<[&str; N]> {
        let mut written = record.splitn(N, FIELD);
        let mut fields = [""; N];
        for field in &mut fields {
            *field = written.next()?;
        }
        Some(fields)
    }
}

/// The `--format=` argument for one [`Format`], as git spells it.
pub(super) struct Pretty<const N: usize>(pub(super) Format<N>);

impl<const N: usize> fmt::Display for Pretty<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("--format=")?;
        f.write_char(RECORD)?;
        for (at, field) in self.0.0.iter().enumerate() {
            if at > 0 {
                f.write_char(FIELD)?;
            }
            f.write_str(field.placeholder())?;
        }
        Ok(())
    }
}

/// One line of a `--name-only` log that says something.
pub(super) enum Entry<'a, const N: usize> {
    /// A commit, as the fields the format asked for.
    Commit([&'a str; N]),
    /// A file the commit above it touched.
    Touched(std::path::PathBuf),
}

impl<'a, const N: usize> Entry<'a, N> {
    /// What `line` is: [`RECORD`] opens a commit and nothing else can, so a
    /// record git wrote that this cannot read is still never taken for a path.
    ///
    /// `None` for a line that says nothing: the blank one git writes between a
    /// commit and its files, a record whose fields do not read, and a quoted
    /// path that does not decode.
    pub(super) fn of(line: &'a str) -> Option<Self> {
        match line.strip_prefix(RECORD) {
            Some(record) => Format::<N>::read(record).map(Self::Commit),
            None if line.is_empty() => None,
            None => Quoted(line).path().map(Self::Touched),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Field, Format, Pretty, RECORD};

    const COMMIT: Format<2> = Format::new([Field::Hash, Field::Committed]);

    /// A line read against [`COMMIT`], whose arity is the whole contract.
    type Line<'a> = Entry<'a, 2>;

    fn record(fields: &[&str]) -> String {
        format!("{RECORD}{}", fields.join("\u{1f}"))
    }

    #[test]
    fn the_argument_opens_a_record_and_names_every_field_in_order() {
        assert_eq!(Pretty(COMMIT).to_string(), "--format=\u{1}%H\u{1f}%cI");
    }

    #[test]
    fn a_record_reads_back_as_the_fields_it_asked_for() {
        let line = record(&["abc", "2026-08-21T10:00:00Z"]);
        let Some(Entry::Commit([hash, committed])) = Line::of(&line) else {
            panic!("a record should read as a commit");
        };

        assert_eq!([hash, committed], ["abc", "2026-08-21T10:00:00Z"]);
    }

    /// The separator has no escape, so a field holding one stays whole instead
    /// of shifting the fields after it.
    #[test]
    fn a_separator_in_the_last_field_stays_there() {
        let line = record(&["abc", "a\u{1f}b"]);
        let Some(Entry::Commit([_, committed])) = Line::of(&line) else {
            panic!("a record should read as a commit");
        };

        assert_eq!(committed, "a\u{1f}b");
    }

    /// A record git wrote that does not read is nothing, and above all not a
    /// path: taking one for a path would credit a commit to a file named after
    /// a commit hash.
    #[test]
    fn a_record_with_too_few_fields_is_neither_a_commit_nor_a_path() {
        assert!(Line::of(&record(&["abc"])).is_none());
    }

    #[test]
    fn anything_that_does_not_open_a_record_is_a_path() {
        let Some(Entry::Touched(path)) = Line::of("content/a.typ") else {
            panic!("a path line should read as a path");
        };

        assert_eq!(path, std::path::Path::new("content/a.typ"));
        assert!(Line::of("").is_none(), "the blank line says nothing");
    }
}
