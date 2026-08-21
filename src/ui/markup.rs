//! The inline markup diagnostics are written in (`` `code` ``, `*bold*`),
//! escaped through [`Text`] and [`Code`] and rendered by [`Styled`].

use std::fmt::{self, Display, Write as _};

use miette::Diagnostic;

/// The character that escapes a delimiter, and itself.
const ESCAPE: char = '\\';

/// A span kind: the delimiter that opens and closes it, and how it renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A path, config key, command, or anything else quoted as literal text.
    Code,
    Bold,
}

impl Kind {
    const ALL: [Self; 2] = [Self::Code, Self::Bold];

    /// The delimiter this kind is written with, and the SGR parameters that
    /// turn its styling on and off; the *off* one is narrow, never a `0` full
    /// reset, which would drop miette's own style for the rest of the string.
    const fn spellings(self) -> (char, &'static str, &'static str) {
        match self {
            Self::Code => ('`', "36", "39"),
            Self::Bold => ('*', "1", "22"),
        }
    }

    const fn delimiter(self) -> char {
        self.spellings().0
    }

    fn of(c: char) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.delimiter() == c)
    }

    /// Whether `c` means something to the parser and so has to be escaped.
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

/// A value interpolated into a diagnostic as literal text, with every markup
/// character in it escaped.
#[derive(Debug, Clone, Copy)]
pub struct Text<T>(pub T);

impl<T: Display> Display for Text<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(Escaping(f), "{}", self.0)
    }
}

/// [`Text`] inside a code span: the one way a diagnostic names a path, a config
/// key, a command or a URL.
#[derive(Debug, Clone, Copy)]
pub struct Code<T>(pub T);

impl<T: Display> Display for Code<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let delimiter = Kind::Code.delimiter();
        write!(f, "{delimiter}{}{delimiter}", Text(&self.0))
    }
}

/// A [`fmt::Write`] that escapes markup on the way through.
///
/// Control characters are dropped rather than escaped: an interpolated value is
/// a page's own text, and one carrying `\x1b[` would otherwise close the span it
/// sits in and restyle the rest of the terminal.
struct Escaping<'a, 'b>(&'a mut fmt::Formatter<'b>);

impl fmt::Write for Escaping<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if c.is_control() && c != '\n' && c != '\t' {
                continue;
            }
            if Kind::is_special(c) {
                self.0.write_char(ESCAPE)?;
            }
            self.0.write_char(c)?;
        }
        Ok(())
    }
}

/// Build a diagnostic string with its interpolated values escaped, for help
/// text assembled at runtime; positional `{}` only, since every argument is
/// wrapped on the way in.
macro_rules! markup {
    ($fmt:literal $(, $arg:expr)* $(,)?) => {
        format!($fmt $(, $crate::ui::Text($arg))*)
    };
}
pub(crate) use markup;

/// Marked-up text, rendered: with `color` a span is styled and its delimiters
/// consumed, without they are written back out.
///
/// The parser is total: an unpaired delimiter, an empty span, or one that would
/// span a line break renders as the literal character.
pub struct Markup<'a> {
    source: &'a str,
    color: bool,
}

impl<'a> Markup<'a> {
    pub fn new(source: &'a str, color: bool) -> Self {
        Self { source, color }
    }

    /// The byte index of the delimiter closing this span, skipping escaped
    /// ones, or `None` when it never closes on this line.
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

    /// Whether the text between two delimiters is a span at all: not empty,
    /// and not padded with whitespace.
    fn is_span(body: &str) -> bool {
        !body.is_empty() && body.trim() == body
    }

    /// Write `body` with one level of `\` removed.
    fn unescaped(body: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            match c {
                ESCAPE => match chars.next() {
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
        let source = self.source;
        let mut literal = 0;
        let mut i = 0;
        while let Some(c) = source[i..].chars().next() {
            let next = i + c.len_utf8();
            if c == ESCAPE {
                i = next + source[next..].chars().next().map_or(0, char::len_utf8);
                continue;
            }
            let Some(kind) = Kind::of(c) else {
                i = next;
                continue;
            };
            let body = &source[next..];
            match Self::close(body, c).filter(|&end| Self::is_span(&body[..end])) {
                Some(end) => {
                    Self::unescaped(&source[literal..i], f)?;
                    self.span(kind, &body[..end], f)?;
                    i = next + end + c.len_utf8();
                    literal = i;
                }
                None => i = next,
            }
        }
        Self::unescaped(&source[literal..], f)
    }
}

/// A [`Diagnostic`] with its markup rendered, wrapped around one just before
/// miette formats it.
///
/// Only the message and the help go through [`Markup`]; a source snippet is
/// forwarded untouched, its backticks being typst or KDL rather than markup.
#[derive(Debug)]
pub struct Styled<'a> {
    inner: &'a dyn Diagnostic,
    /// Wrapped eagerly, so [`Diagnostic::related`] has something borrowable to
    /// hand back; `None` and `Some(vec![])` are different answers to miette.
    related: Option<Vec<Self>>,
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
        let source = "failed to read `content/post.typ`";
        assert_eq!(plain(source), source);
    }

    #[test]
    fn a_code_span_styles_and_drops_its_delimiters() {
        assert_eq!(color("read `a.typ`"), "read \x1b[36ma.typ\x1b[39m");
    }

    #[test]
    fn every_kind_renders_with_a_narrow_reset() {
        assert_eq!(color("*b*"), "\x1b[1mb\x1b[22m");
        assert_eq!(color("`c`"), "\x1b[36mc\x1b[39m");
    }

    #[test]
    fn an_interpolated_value_carries_no_control_character_through() {
        let hostile = "en\u{1b}[41m BREACHED \u{1b}[0m";
        assert_eq!(Text(hostile).to_string(), "en[41m BREACHED [0m");
        assert_eq!(Code(hostile).to_string(), "`en[41m BREACHED [0m`");
    }

    #[test]
    fn an_unpaired_delimiter_is_literal() {
        assert_eq!(color("2 * 3 and `x"), "2 * 3 and `x");
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
        assert_eq!(plain(r"end\"), r"end\");
    }

    #[test]
    fn an_escaped_delimiter_does_not_close_a_span() {
        assert_eq!(color(r"`a\`b`"), "\x1b[36ma`b\x1b[39m");
    }

    #[test]
    fn spans_do_not_nest() {
        assert_eq!(color("`a *b* c`"), "\x1b[36ma *b* c\x1b[39m");
    }

    #[test]
    fn interpolated_values_cannot_open_a_span() {
        let hostile = "a*b`c";
        let message = format!("read {}", Code(hostile));
        assert_eq!(color(&message), format!("read \x1b[36m{hostile}\x1b[39m"));
        assert_eq!(plain(&message), format!("read `{hostile}`"));
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

    /// A help built from another crate's error text: the one place a diagnostic
    /// carries prose this crate did not write, and so the one that has to go
    /// through [`Text`] rather than be interpolated raw.
    #[test]
    fn foreign_text_in_a_help_cannot_restyle_the_line() {
        let foreign = "regex parse error:\n    *a`b*\n    ^\nerror: repetition operator";
        let escaped = Text(foreign).to_string();

        assert!(
            color(foreign).contains('\u{1b}'),
            "raw foreign text opens a span: {:?}",
            color(foreign)
        );
        assert_eq!(
            color(&escaped),
            foreign,
            "escaped, it renders as itself and styles nothing"
        );
    }

    #[test]
    fn multibyte_text_around_a_span_keeps_its_boundaries() {
        assert_eq!(color("é `a` ü"), "é \x1b[36ma\x1b[39m ü");
    }

    /// A diagnostic with one of everything [`Styled`] has to deal with.
    #[derive(Debug)]
    struct Fake {
        related: Vec<Self>,
    }

    impl Display for Fake {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("read `a.typ`")
        }
    }

    impl std::error::Error for Fake {}

    impl Diagnostic for Fake {
        fn code(&self) -> Option<Box<dyn Display + '_>> {
            Some(Box::new("test::fake"))
        }

        fn help(&self) -> Option<Box<dyn Display + '_>> {
            Some(Box::new("try `b.typ`"))
        }

        fn severity(&self) -> Option<miette::Severity> {
            Some(miette::Severity::Warning)
        }

        fn source_code(&self) -> Option<&dyn miette::SourceCode> {
            Some(&"#let x = `raw`" as &dyn miette::SourceCode)
        }

        fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
            Some(Box::new(std::iter::once(miette::LabeledSpan::new(
                None, 0, 4,
            ))))
        }

        fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
            (!self.related.is_empty())
                .then(|| Box::new(self.related.iter().map(|d| d as &dyn Diagnostic)) as _)
        }
    }

    fn fake() -> Fake {
        Fake {
            related: vec![Fake { related: vec![] }],
        }
    }

    #[test]
    fn styled_renders_the_message_and_the_help() {
        let fake = fake();
        let styled = Styled::new(&fake, true);
        assert_eq!(styled.to_string(), "read \x1b[36ma.typ\x1b[39m");
        assert_eq!(
            styled.help().expect("a help").to_string(),
            "try \x1b[36mb.typ\x1b[39m"
        );
    }

    #[test]
    fn styled_forwards_the_rest_and_wraps_related() {
        let fake = fake();
        let styled = Styled::new(&fake, true);
        assert_eq!(styled.code().expect("a code").to_string(), "test::fake");
        assert_eq!(styled.severity(), Some(miette::Severity::Warning));
        let related: Vec<_> = styled
            .related()
            .expect("one related")
            .map(ToString::to_string)
            .collect();
        assert_eq!(related, ["read \x1b[36ma.typ\x1b[39m"]);
        let leaf = Fake { related: vec![] };
        assert!(Styled::new(&leaf, true).related().is_none());
    }

    #[test]
    fn styled_leaves_the_source_snippet_alone() {
        let fake = Fake { related: vec![] };
        let mut report = String::new();
        miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::none())
            .render_report(&mut report, &Styled::new(&fake, true))
            .expect("renders");
        assert!(report.contains("read \x1b[36ma.typ\x1b[39m"), "{report}");
        assert!(report.contains("#let x = `raw`"), "{report}");
    }

    /// Whether `body` holds the named-field shorthand thiserror and miette
    /// both accept (`{path}`, `{0:?}`).
    fn shorthand(body: &str) -> bool {
        let name = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let bytes = body.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'{' {
                i += 1;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && name(bytes[end]) {
                end += 1;
            }
            if end > start && matches!(bytes.get(end), Some(b'}' | b':')) {
                return true;
            }
            i = start;
        }
        false
    }

    /// Everything between a pair of backticks.
    fn spans(text: &str) -> impl Iterator<Item = &str> {
        text.split('`').skip(1).step_by(2)
    }

    /// A Rust source file, walked for the text a reader will see rendered as
    /// markup: the string literals inside an `#[error(..)]`, a `help(..)`, or a
    /// [`markup!`] call.
    struct Scan<'a> {
        text: &'a str,
        /// Whether every string literal counts, rather than only those inside
        /// a [`Scan::TRIGGERS`] construct; true under `src/error/`.
        everything: bool,
        at: usize,
        line: usize,
        /// Unclosed parens of the construct walked, zero when outside one.
        depth: usize,
        found: Vec<(usize, String)>,
    }

    impl<'a> Scan<'a> {
        /// The constructs whose text reaches a reader through [`Styled`].
        const TRIGGERS: &'static [&'static str] = &["#[error(", "help(", "markup!("];

        /// Where the walk stops: a test module's literals are assertions, not
        /// output.
        const TESTS: &'static str = "#[cfg(test)]";

        fn new(text: &'a str, everything: bool) -> Self {
            Self {
                text,
                everything,
                at: 0,
                line: 1,
                depth: 0,
                found: Vec::new(),
            }
        }

        /// Every literal that will be rendered as markup, as `(line, text)`.
        fn messages(mut self) -> Vec<(usize, String)> {
            let b = self.text.as_bytes();
            while self.at < b.len() {
                let next = b.get(self.at + 1).copied();
                match b[self.at] {
                    b'\n' => {
                        self.line += 1;
                        self.at += 1;
                    }
                    b'/' if next == Some(b'/') => self.skip_line(),
                    b'/' if next == Some(b'*') => self.block_comment(),
                    b'"' => self.literal(0),
                    b'r' => match (!self.in_word()).then(|| self.raw()).flatten() {
                        Some(hashes) => self.literal(hashes),
                        None => self.step(),
                    },
                    b'(' if self.depth > 0 => {
                        self.depth += 1;
                        self.at += 1;
                    }
                    b')' if self.depth > 0 => {
                        self.depth -= 1;
                        self.at += 1;
                    }
                    _ => self.step(),
                }
            }
            self.found
        }

        /// One byte of ordinary code: enters a construct when one starts here,
        /// and stops the walk at a test module.
        fn step(&mut self) {
            let rest = &self.text.as_bytes()[self.at..];
            if rest.starts_with(Self::TESTS.as_bytes()) {
                self.at = self.text.len();
                return;
            }
            if self.depth == 0
                && let Some(trigger) = Self::TRIGGERS
                    .iter()
                    .find(|t| rest.starts_with(t.as_bytes()))
            {
                self.depth = 1;
                self.at += trigger.len();
                return;
            }
            self.at += 1;
        }

        /// The number of `#`s in a raw-string opener starting here, if this is
        /// one.
        fn raw(&self) -> Option<usize> {
            let b = self.text.as_bytes();
            let mut i = self.at + 1;
            while b.get(i) == Some(&b'#') {
                i += 1;
            }
            (b.get(i) == Some(&b'"')).then_some(i - self.at - 1)
        }

        /// Whether the byte before this one continues an identifier, so an `r`
        /// is the tail of a name rather than a raw-string prefix.
        fn in_word(&self) -> bool {
            let Some(prev) = self.at.checked_sub(1).map(|i| self.text.as_bytes()[i]) else {
                return false;
            };
            prev.is_ascii_alphanumeric() || prev == b'_'
        }

        /// Read the string literal starting here, recording it when it is
        /// diagnostic text. `hashes` is zero for an ordinary literal.
        fn literal(&mut self, hashes: usize) {
            let b = self.text.as_bytes();
            let raw = b[self.at] == b'r';
            let start = self.at + usize::from(raw) + hashes + 1;
            let opened = self.line;
            let mut i = start;
            while i < b.len() {
                match b[i] {
                    b'\\' if !raw => {
                        self.line += usize::from(b.get(i + 1) == Some(&b'\n'));
                        i += 2;
                    }
                    b'\n' => {
                        self.line += 1;
                        i += 1;
                    }
                    b'"' if b[i + 1..].iter().take(hashes).all(|c| *c == b'#') => break,
                    _ => i += 1,
                }
            }
            let end = i.min(b.len());
            if self.everything || self.depth > 0 {
                self.found.push((opened, self.text[start..end].to_owned()));
            }
            self.at = (end + hashes + 1).min(b.len());
        }

        fn skip_line(&mut self) {
            let b = self.text.as_bytes();
            while self.at < b.len() && b[self.at] != b'\n' {
                self.at += 1;
            }
        }

        fn block_comment(&mut self) {
            let b = self.text.as_bytes();
            self.at += 2;
            while self.at < b.len() && !b[self.at..].starts_with(b"*/") {
                if b[self.at] == b'\n' {
                    self.line += 1;
                }
                self.at += 1;
            }
            self.at = (self.at + 2).min(b.len());
        }
    }

    /// Every `.rs` file under `src/`, with the path each was read from.
    fn sources() -> Vec<std::path::PathBuf> {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("a readable directory") {
                let path = entry.expect("a readable entry").path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|n| n == "tests") {
                        continue;
                    }
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
            &mut out,
        );
        out
    }

    /// The rule [`Text`] and [`Code`] exist to enforce: a value interpolated
    /// inside a code span goes through an adapter, never through the `{field}`
    /// shorthand.
    #[test]
    fn no_diagnostic_interpolates_a_raw_value_inside_a_code_span() {
        let mut offenders = Vec::new();
        for path in sources() {
            let text = std::fs::read_to_string(&path).expect("readable source");
            let everything = path.components().any(|c| c.as_os_str() == "error");
            for (line, message) in Scan::new(&text, everything).messages() {
                if spans(&message).any(shorthand) {
                    let name = path.display();
                    offenders.push(format!("{name}:{line}: {message}"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "interpolate through `Code(.field)` or `Text(.field)` instead:\n{}",
            offenders.join("\n")
        );
    }

    #[test]
    fn the_check_reads_the_constructs_and_not_the_lines() {
        let scan = |text: &str| {
            Scan::new(text, false)
                .messages()
                .into_iter()
                .filter(|(_, m)| spans(m).any(shorthand))
                .count()
        };
        assert_eq!(scan("#[error(\"read `{path}`\")]"), 1);
        assert_eq!(
            scan("#[diagnostic(\n  code(x::y),\n  help(\"try `{other}`\")\n)]"),
            1
        );
        assert_eq!(scan("let h = markup!(\"try `{guess}`\");"), 1);
        assert_eq!(scan("#[error(\"read {}\", Code(.path))]"), 0);
        assert_eq!(scan("markup!(\"did you mean `{}`?\", guess)"), 0);
        assert_eq!(scan("#[error(\"write `assets {{ tsconfig }}`\")]"), 0);
        assert_eq!(scan("let s = \"a `{name}` label\";"), 0);
        assert_eq!(
            Scan::new("let s = \"a `{name}` label\";", true)
                .messages()
                .into_iter()
                .filter(|(_, m)| spans(m).any(shorthand))
                .count(),
            1
        );
    }
}
