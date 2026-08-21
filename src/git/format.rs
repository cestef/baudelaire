//! The record contract: what `git` is asked to print about a commit, and how
//! what it printed is read back.
//!
//! One [`Format`] states both, so a field added to the request is a field the
//! reader has to destructure.

use std::fmt::{self, Write as _};

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
}

impl Field {
    const fn placeholder(self) -> &'static str {
        match self {
            Self::Hash => "%H",
            Self::Committed => "%cI",
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
    pub(super) fn read(record: &str) -> Option<[&str; N]> {
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
        for (at, field) in self.0.0.iter().enumerate() {
            if at > 0 {
                f.write_char(FIELD)?;
            }
            f.write_str(field.placeholder())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Field, Format, Pretty};

    const COMMIT: Format<2> = Format::new([Field::Hash, Field::Committed]);

    #[test]
    fn the_argument_names_every_field_in_order() {
        assert_eq!(Pretty(COMMIT).to_string(), "--format=%H\u{1f}%cI");
    }

    #[test]
    fn a_record_reads_back_as_the_fields_it_asked_for() {
        assert_eq!(
            Format::<2>::read("abc\u{1f}2026-08-21T10:00:00Z"),
            Some(["abc", "2026-08-21T10:00:00Z"])
        );
    }

    /// The separator has no escape, so a field holding one stays whole instead
    /// of shifting the fields after it.
    #[test]
    fn a_separator_in_the_last_field_stays_there() {
        assert_eq!(
            Format::<2>::read("abc\u{1f}a\u{1f}b"),
            Some(["abc", "a\u{1f}b"])
        );
    }

    #[test]
    fn a_record_with_too_few_fields_is_not_one() {
        assert_eq!(Format::<2>::read("abc"), None);
    }
}
