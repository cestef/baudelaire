//! The inline markup diagnostics are written in, and the one place it is
//! escaped and rendered.
//!
//! Every error and warning message in [`crate::error`] already reads as marked
//! up prose: a path, a config key or a command sits in `backticks`. This module
//! makes that a real grammar with a renderer behind it, and closes the hole it
//! left open, which is that the values interpolated into those messages are
//! user data. A page slug named `a*b` used to be able to open a span the author
//! never wrote.
//!
//! Three pieces, and they have to agree, so they live together:
//!
//! - **The grammar.** [`Kind`] is the one table of delimiters and the styling
//!   each carries: `` `code` ``, `*bold*`, `_italic_`, with `\` escaping any of
//!   them. Spans do not nest and do not cross a line.
//! - **Escaping.** [`Text`] and [`Code`] are the Display adapters an error
//!   interpolates a value through, so the value cannot be read as markup.
//!   [`markup!`] does the same for the help strings that are assembled at
//!   runtime rather than written in an attribute.
//! - **Rendering.** [`Styled`] wraps a [`Diagnostic`] just before miette sees
//!   it and rewrites its message and help through [`Markup`], leaving code,
//!   labels and source snippets alone.
//!
//! Rendering is deliberately *structural* rather than a matter of stripping
//! colour afterwards: with colour, a span is styled and its delimiters are
//! consumed; without, the delimiters are written back out. So piped output is
//! exactly the text these messages already produced before any of this existed.

use std::fmt::{self, Display, Write as _};

use miette::Diagnostic;

/// The character that escapes a delimiter, and itself.
const ESCAPE: char = '\\';

/// A span kind: the delimiter that opens and closes it, and how it renders.
///
/// The one table. A new kind is a variant plus a row in [`Kind::spellings`];
/// the parser, the escaper and the plain-text renderer all derive from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A path, config key, command, or anything else quoted as literal text.
    Code,
    Bold,
    Italic,
}

impl Kind {
    /// Every kind, so the escaper and the parser cannot fall out of step with
    /// the table by listing delimiters of their own.
    const ALL: [Self; 3] = [Self::Code, Self::Bold, Self::Italic];

    /// The delimiter this kind is written with, and the SGR parameters that
    /// turn its styling on and off.
    ///
    /// The *off* parameter is the narrow one (`22` for bold, `39` for a
    /// foreground colour), never a `0` full reset: miette applies its own style
    /// to the whole of a help string, and a full reset inside would drop that
    /// style for everything after the span.
    const fn spellings(self) -> (char, &'static str, &'static str) {
        match self {
            Self::Code => ('`', "36", "39"),
            Self::Bold => ('*', "1", "22"),
            Self::Italic => ('_', "3", "23"),
        }
    }

    const fn delimiter(self) -> char {
        self.spellings().0
    }

    fn of(c: char) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.delimiter() == c)
    }

    /// Whether `c` means something to the parser and so has to be escaped when
    /// it arrives inside an interpolated value.
    fn is_special(c: char) -> bool {
        c == ESCAPE || Self::of(c).is_some()
    }

    fn open(self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\x1b[{}m", self.spellings().1)
    }

    fn close(self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\x1b[{}m", self.spellings().2)
    }
}

/// A value interpolated into a diagnostic as literal text: every markup
/// character in it is escaped, so a path named `a*b` cannot open a span its
/// author never wrote.
///
/// Used inside a delimiter the message already writes:
/// ``help("run `ssh-keygen -R {}`", Text(.host))``.
pub struct Text<T>(pub T);

impl<T: Display> Display for Text<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(Escaping(f), "{}", self.0)
    }
}

/// [`Text`] inside a code span: the one way a diagnostic names a path, a config
/// key, a command or a URL.
///
/// ``#[error("failed to {op} {}", Code(.path))]`` renders as ``failed to read
/// `content/post.typ` ``, with the value escaped.
pub struct Code<T>(pub T);

impl<T: Display> Display for Code<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let delimiter = Kind::Code.delimiter();
        write!(f, "{delimiter}{}{delimiter}", Text(&self.0))
    }
}

/// A [`fmt::Write`] that escapes markup on the way through, so [`Text`] can
/// forward a value straight into the formatter instead of rendering it to a
/// `String` first only to walk it again.
struct Escaping<'a, 'b>(&'a mut fmt::Formatter<'b>);

impl fmt::Write for Escaping<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if Kind::is_special(c) {
                self.0.write_char(ESCAPE)?;
            }
            self.0.write_char(c)?;
        }
        Ok(())
    }
}

/// Build a diagnostic string with its interpolated values escaped, for the help
/// text that is assembled at runtime rather than written in an attribute (see
/// [`crate::error::Annotated`]).
///
/// The literal owns the delimiters and the arguments are always plain text:
/// ``markup!("did you mean `{}`?", guess)``. Positional `{}` only, since every
/// argument is wrapped on the way in.
macro_rules! markup {
    ($fmt:literal $(, $arg:expr)* $(,)?) => {
        format!($fmt $(, $crate::ui::Text($arg))*)
    };
}
pub(crate) use markup;

/// Marked-up text, rendered.
///
/// With `color`, a span is styled and its delimiters are consumed; without, the
/// delimiters are written back out, which is the plain rendering these messages
/// have always had. Either way the escapes are resolved, so a value that
/// arrived through [`Text`] reads as itself.
///
/// The parser is total: an unpaired delimiter, an empty span, or one that would
/// span a line break is not markup and renders as the literal character. A
/// malformed message degrades to plain text rather than failing a build.
pub struct Markup<'a> {
    source: &'a str,
    color: bool,
}

impl<'a> Markup<'a> {
    pub fn new(source: &'a str, color: bool) -> Self {
        Self { source, color }
    }

    /// The byte index of the delimiter closing a span opened at `from`, or
    /// `None` if the span never closes on this line. Escaped delimiters are
    /// skipped, so `` `a\`b` `` closes at the third backtick.
    fn close(rest: &str, delimiter: char) -> Option<usize> {
        let mut chars = rest.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                ESCAPE => {
                    chars.next();
                }
                '\n' => return None,
                _ if c == delimiter => return Some(i),
                _ => {}
            }
        }
        None
    }

    /// Whether the text between two delimiters is a span at all. Empty is not,
    /// and neither is one padded with whitespace: `a * b * c` is arithmetic, or
    /// prose, not bold.
    fn is_span(body: &str) -> bool {
        !body.is_empty() && body.trim() == body
    }

    /// Write `body` with its escapes resolved: what the author meant, one level
    /// of `\` removed.
    fn unescaped(body: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            match c {
                ESCAPE => match chars.next() {
                    // A trailing lone backslash is not an escape; it is a
                    // backslash.
                    None => f.write_char(ESCAPE)?,
                    Some(next) => f.write_char(next)?,
                },
                _ => f.write_char(c)?,
            }
        }
        Ok(())
    }

    fn span(&self, kind: Kind, body: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.color {
            kind.open(f)?;
            Self::unescaped(body, f)?;
            kind.close(f)
        } else {
            let delimiter = kind.delimiter();
            f.write_char(delimiter)?;
            Self::unescaped(body, f)?;
            f.write_char(delimiter)
        }
    }
}

impl Display for Markup<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut rest = self.source;
        // The run of literal text before the next span, flushed whole so a
        // message with no markup costs one write.
        let mut literal = 0;
        while let Some((offset, c)) = rest[literal..]
            .char_indices()
            .map(|(i, c)| (literal + i, c))
            .find(|&(_, c)| Kind::is_special(c))
        {
            let body = &rest[offset + c.len_utf8()..];
            let span = Kind::of(c)
                .zip(Self::close(body, c))
                .filter(|&(_, end)| Self::is_span(&body[..end]));
            match span {
                Some((kind, end)) => {
                    Self::unescaped(&rest[..offset], f)?;
                    self.span(kind, &body[..end], f)?;
                    rest = &body[end + c.len_utf8()..];
                    literal = 0;
                }
                // Not a span: an escape, or a delimiter that never closes.
                // Leave it in the literal run, which `unescaped` resolves.
                None => literal = offset + c.len_utf8() + usize::from(c == ESCAPE),
            }
        }
        Self::unescaped(rest, f)
    }
}

/// A [`Diagnostic`] with its markup rendered, wrapped around one just before
/// miette formats it.
///
/// Only the message and the help go through [`Markup`]. Everything else is
/// forwarded untouched, and the source snippet especially: a config error
/// carries KDL and a compile error carries typst, both full of backticks and
/// asterisks that are code, not markup.
///
/// `related` is wrapped recursively, because that is where this crate's own
/// aggregates live (a build that failed on three pages, a typst compile with
/// several diagnostics). `diagnostic_source` is not: it is the handoff to a
/// foreign diagnostic (kdl's), whose text was never written in this grammar.
#[derive(Debug)]
pub struct Styled<'a> {
    inner: &'a dyn Diagnostic,
    /// Wrapped eagerly, so [`Diagnostic::related`] has something borrowable to
    /// hand back. `None` and `Some(vec![])` are different answers to miette.
    related: Option<Vec<Styled<'a>>>,
    color: bool,
}

impl<'a> Styled<'a> {
    pub fn new(inner: &'a dyn Diagnostic, color: bool) -> Self {
        let related = inner
            .related()
            .map(|it| it.map(|d| Self::new(d, color)).collect());
        Self {
            inner,
            related,
            color,
        }
    }

    fn render(&self, text: &dyn Display) -> String {
        Markup::new(&text.to_string(), self.color).to_string()
    }
}

impl Display for Styled<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Markup::new(&self.inner.to_string(), self.color).fmt(f)
    }
}

impl std::error::Error for Styled<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source()
    }
}

impl Diagnostic for Styled<'_> {
    fn help(&self) -> Option<Box<dyn Display + '_>> {
        let help = self.inner.help()?;
        Some(Box::new(self.render(&help)))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
        let related = self.related.as_ref()?;
        Some(Box::new(related.iter().map(|s| s as &dyn Diagnostic)))
    }

    fn code(&self) -> Option<Box<dyn Display + '_>> {
        self.inner.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.inner.severity()
    }

    fn url(&self) -> Option<Box<dyn Display + '_>> {
        self.inner.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.inner.source_code()
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.inner.labels()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.inner.diagnostic_source()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(source: &str) -> String {
        Markup::new(source, true).to_string()
    }

    fn plain(source: &str) -> String {
        Markup::new(source, false).to_string()
    }

    #[test]
    fn plain_rendering_is_the_text_as_written() {
        // The pre-existing rendering: piped output must not change.
        let source = "failed to read `content/post.typ`";
        assert_eq!(plain(source), source);
    }

    #[test]
    fn a_code_span_styles_and_drops_its_delimiters() {
        assert_eq!(color("read `a.typ`"), "read \x1b[36ma.typ\x1b[39m");
    }

    #[test]
    fn every_kind_renders_with_a_narrow_reset() {
        // Never `\x1b[0m`: miette styles the whole help string around us.
        assert_eq!(color("*b*"), "\x1b[1mb\x1b[22m");
        assert_eq!(color("_i_"), "\x1b[3mi\x1b[23m");
        assert_eq!(color("`c`"), "\x1b[36mc\x1b[39m");
    }

    #[test]
    fn an_unpaired_delimiter_is_literal() {
        assert_eq!(color("2 * 3 and `x"), "2 * 3 and `x");
        assert_eq!(plain("a_b"), "a_b");
    }

    #[test]
    fn a_padded_or_empty_span_is_not_a_span() {
        assert_eq!(color("2 * 3 * 4"), "2 * 3 * 4");
        assert_eq!(color("``"), "``");
    }

    #[test]
    fn a_span_does_not_cross_a_line() {
        assert_eq!(color("`a\nb`"), "`a\nb`");
    }

    #[test]
    fn escapes_resolve_in_both_modes() {
        assert_eq!(color(r"a \* b"), "a * b");
        assert_eq!(plain(r"a \* b"), "a * b");
        assert_eq!(color(r"\\"), r"\");
        // A trailing lone backslash is a backslash.
        assert_eq!(plain(r"end\"), r"end\");
    }

    #[test]
    fn an_escaped_delimiter_does_not_close_a_span() {
        assert_eq!(color(r"`a\`b`"), "\x1b[36ma`b\x1b[39m");
    }

    #[test]
    fn spans_do_not_nest() {
        // The inner delimiter is literal text inside the code span.
        assert_eq!(color("`a *b* c`"), "\x1b[36ma *b* c\x1b[39m");
    }

    #[test]
    fn interpolated_values_cannot_open_a_span() {
        let hostile = "a*b_c`d";
        let message = format!("read {}", Code(hostile));
        assert_eq!(color(&message), format!("\x1b[36m{hostile}\x1b[39m"));
        assert_eq!(plain(&message), format!("`{hostile}`"));
    }

    #[test]
    fn text_escapes_without_adding_a_span() {
        assert_eq!(Text("a*b").to_string(), r"a\*b");
        assert_eq!(Code("a*b").to_string(), r"`a\*b`");
        assert_eq!(Text(r"c:\x").to_string(), r"c:\\x");
    }

    #[test]
    fn the_macro_escapes_its_arguments() {
        let rendered = markup!("did you mean `{}`?", "a*b");
        assert_eq!(color(&rendered), "did you mean \x1b[36ma*b\x1b[39m?");
    }

    #[test]
    fn multibyte_text_around_a_span_keeps_its_boundaries() {
        assert_eq!(color("é `a` ü"), "é \x1b[36ma\x1b[39m ü");
    }
}
