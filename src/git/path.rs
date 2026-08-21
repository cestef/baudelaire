//! A path as git wrote it. Git quotes one it cannot write raw, and the quoting
//! is C's, so the reader has to be too.

use std::path::PathBuf;

/// A path in a `git` listing: bare, or double-quoted with C escapes where it
/// holds a byte git will not write as itself.
///
/// `core.quotepath=false` keeps a non-ASCII path bare, so what remains quoted is
/// a path holding a quote, a backslash, or a control character.
pub(super) struct Quoted<'a>(pub(super) &'a str);

impl Quoted<'_> {
    /// The path itself.
    ///
    /// `None` for a quoted path whose escapes do not decode to UTF-8, which
    /// names no page: a page's path has to be UTF-8 to be a Typst file id at
    /// all.
    pub(super) fn path(&self) -> Option<PathBuf> {
        let Some(escaped) = self
            .0
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        else {
            return Some(PathBuf::from(self.0));
        };
        String::from_utf8(Self::unescape(escaped)?)
            .ok()
            .map(PathBuf::from)
    }

    /// The bytes `escaped` stands for, or `None` where it ends inside an
    /// escape.
    ///
    /// Octal is a *byte*, not a character: git escapes each byte of a name it
    /// cannot write, so a run of them decodes together and is read as UTF-8
    /// afterwards.
    fn unescape(escaped: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(escaped.len());
        let mut bytes = escaped.bytes();
        while let Some(byte) = bytes.next() {
            if byte != b'\\' {
                out.push(byte);
                continue;
            }
            match bytes.next()? {
                b'a' => out.push(0x07),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0c),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'v' => out.push(0x0b),
                octal @ b'0'..=b'7' => out.push(Self::octal(octal, &mut bytes)?),
                literal => out.push(literal),
            }
        }
        Some(out)
    }

    /// The byte a `\nnn` escape stands for, its first digit already read.
    ///
    /// Git always writes three digits, so the two that follow belong to the
    /// escape and are never text.
    fn octal(first: u8, rest: &mut std::str::Bytes<'_>) -> Option<u8> {
        let mut value = u32::from(first - b'0');
        for _ in 0..2 {
            let digit = rest.next()?;
            if !digit.is_ascii_digit() || digit > b'7' {
                return None;
            }
            value = value * 8 + u32::from(digit - b'0');
        }
        u8::try_from(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::Quoted;
    use std::path::PathBuf;

    fn path(written: &str) -> Option<PathBuf> {
        Quoted(written).path()
    }

    #[test]
    fn a_bare_path_is_itself() {
        assert_eq!(
            path("content/posts/a.typ"),
            Some("content/posts/a.typ".into())
        );
    }

    /// `core.quotepath=false` leaves an accented name bare, and the reader must
    /// not mistake it for something to decode.
    #[test]
    fn a_bare_path_may_hold_anything_that_is_not_a_quote() {
        assert_eq!(path("content/café.typ"), Some("content/café.typ".into()));
    }

    #[test]
    fn a_quoted_path_gives_back_what_was_escaped() {
        assert_eq!(path(r#""content/a b.typ""#), Some("content/a b.typ".into()));
        assert_eq!(
            path(r#""content/a\"b.typ""#),
            Some(r#"content/a"b.typ"#.into())
        );
        assert_eq!(
            path(r#""content/a\\b.typ""#),
            Some(r"content/a\b.typ".into())
        );
        assert_eq!(
            path(r#""content/a\nb.typ""#),
            Some("content/a\nb.typ".into())
        );
        assert_eq!(
            path(r#""content/a\tb.typ""#),
            Some("content/a\tb.typ".into())
        );
    }

    /// What git writes for `café.typ` with `core.quotepath` left on: one octal
    /// escape per byte of the character, which only decode as UTF-8 together.
    #[test]
    fn octal_escapes_decode_as_bytes_and_not_as_characters() {
        assert_eq!(
            path(r#""content/caf\303\251.typ""#),
            Some("content/café.typ".into())
        );
    }

    #[test]
    fn a_path_that_does_not_decode_names_no_page() {
        assert_eq!(path(r#""content/\377.typ""#), None, "not UTF-8");
        assert_eq!(path(r#""content/a\""#), None, "ends inside an escape");
        assert_eq!(path(r#""content/a\12.typ""#), None, "a short octal escape");
    }
}
