//! Display helpers: the single source of count, size, duration, path, and
//! wall-clock formatting across the CLI.

use std::fmt::Display;
use std::time::Duration;

use owo_colors::OwoColorize;
use time::OffsetDateTime;

/// Displays a count with a pluralized noun: `1 page` / `3 pages`.
pub struct Count {
    n: usize,
    noun: &'static str,
}

impl Count {
    pub fn pages(n: usize) -> Self {
        Self { n, noun: "page" }
    }

    pub fn documents(n: usize) -> Self {
        Self {
            n,
            noun: "document",
        }
    }

    pub fn redirects(n: usize) -> Self {
        Self {
            n,
            noun: "redirect",
        }
    }

    pub fn assets(n: usize) -> Self {
        Self { n, noun: "asset" }
    }

    pub fn files(n: usize) -> Self {
        Self { n, noun: "file" }
    }

    pub fn statics(n: usize) -> Self {
        Self {
            n,
            noun: "static file",
        }
    }

    /// A count whose noun the caller supplies, pluralized here like the rest.
    pub fn of(n: usize, noun: &'static str) -> Self {
        Self { n, noun }
    }

    pub fn warnings(n: usize) -> Self {
        Self { n, noun: "warning" }
    }

    /// The noun with its plural `s`.
    fn label(&self) -> String {
        format!("{}{}", self.noun, if self.n == 1 { "" } else { "s" })
    }

    pub fn styled(&self) -> StyledCount<'_> {
        StyledCount(self)
    }
}

impl Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.n, self.label())
    }
}

/// [`Count`] with the number bold and the label dimmed.
pub struct StyledCount<'a>(&'a Count);

impl Display for StyledCount<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.0.n.bold(), self.0.label().dimmed())
    }
}

/// A byte count in binary units (`512 B`, `1.4 MiB`), 1024-based with one
/// decimal above the byte threshold.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bytes(pub u64);

impl Bytes {
    /// The units a size may be written in, largest first so `MB` is matched
    /// before `B`; the `i` spellings mean the same, these being 1024-based.
    const UNITS: &'static [(&'static str, u32)] = &[
        ("gib", 1 << 30),
        ("mib", 1 << 20),
        ("kib", 1 << 10),
        ("gb", 1 << 30),
        ("mb", 1 << 20),
        ("kb", 1 << 10),
        ("g", 1 << 30),
        ("m", 1 << 20),
        ("k", 1 << 10),
        ("b", 1),
    ];

    /// A size written as a number and an optional unit (`0`, `500`, `50kB`,
    /// `1.5 MB`), or `None` when it is neither. Case and inner spaces are
    /// ignored; a bare number is bytes.
    // The float math is what scales `1.5 MB`, and the product is range-checked
    // before it narrows back.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase().replace(' ', "");
        let (number, scale) = Self::UNITS
            .iter()
            .find_map(|&(unit, scale)| Some((text.strip_suffix(unit)?, scale)))
            .unwrap_or((text.as_str(), 1));
        let value: f64 = number.parse().ok()?;
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        let bytes = value * f64::from(scale);
        (bytes <= u64::MAX as f64).then_some(Self(bytes as u64))
    }

    /// The scaled number and its unit.
    // Sizes past 2^53 bytes lose their last digits, and the scaled form shows
    // one decimal anyway.
    #[allow(clippy::cast_precision_loss)]
    fn parts(self) -> (String, &'static str) {
        const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
        let mut size = self.0 as f64;
        let mut unit = 0;
        while size >= 1024.0 && unit < UNITS.len() - 1 {
            size /= 1024.0;
            unit += 1;
        }
        let value = if unit == 0 {
            self.0.to_string()
        } else {
            format!("{size:.1}")
        };
        (value, UNITS[unit])
    }

    pub fn styled(&self) -> StyledBytes<'_> {
        StyledBytes(self)
    }
}

impl Display for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (value, unit) = self.parts();
        write!(f, "{value} {unit}")
    }
}

/// [`Bytes`] with the number bold and the unit dimmed.
pub struct StyledBytes<'a>(&'a Bytes);

impl Display for StyledBytes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (value, unit) = self.0.parts();
        write!(f, "{} {}", value.bold(), unit.dimmed())
    }
}

/// A duration in the tightest sensible unit (`840µs`, `132ms`, `1.24s`).
pub struct Dur(pub Duration);

impl Dur {
    /// The units a duration may be written in, longest spelling first so `ms`
    /// is matched before the `s` inside it.
    const UNITS: &'static [(&'static str, u64)] = &[
        ("ms", 1),
        ("s", 1_000),
        ("m", 60 * 1_000),
        ("h", 60 * 60 * 1_000),
        ("d", 24 * 60 * 60 * 1_000),
    ];

    /// A duration written as a number and an optional unit (`0`, `30`, `10s`,
    /// `7d`, `1.5h`), or `None` when it is neither. Case and inner spaces are
    /// ignored; a bare number is seconds, the unit a config reaches for most.
    /// Deliberately not the inverse of this type's [`Display`], which renders
    /// an elapsed time for a reader (`1m 15s`) rather than a single value.
    // The float math is what scales `1.5h`, and the product is range-checked
    // before it narrows back.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase().replace(' ', "");
        let (number, scale) = Self::UNITS
            .iter()
            .find_map(|&(unit, scale)| Some((text.strip_suffix(unit)?, scale)))
            .unwrap_or((text.as_str(), 1_000));
        let value: f64 = number.parse().ok()?;
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        let millis = value * scale as f64;
        (millis <= u64::MAX as f64).then(|| Self(Duration::from_millis(millis as u64)))
    }
}

impl Display for Dur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let d = self.0;
        if d < Duration::from_millis(1) {
            write!(f, "{}µs", d.as_micros())
        } else if d < Duration::from_secs(1) {
            write!(f, "{}ms", d.as_millis())
        } else if d < Duration::from_mins(1) {
            write!(f, "{:.2}s", d.as_secs_f64())
        } else {
            let secs = d.as_secs();
            write!(f, "{}m {:02}s", secs / 60, secs % 60)
        }
    }
}

/// Renders a path with its directory portion dimmed and its final component in
/// cyan.
pub struct Paths<'a>(pub &'a str);

impl Display for Paths<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.rfind('/') {
            Some(i) => {
                let (dir, file) = self.0.split_at(i + 1);
                write!(f, "{}{}", dir.dimmed(), file.cyan())
            }
            None => write!(f, "{}", self.0.cyan()),
        }
    }
}

/// Renders items as prose: `a`, `a and b`, `a, b and c`.
pub struct List<'a, T>(pub &'a [T]);

impl<T: Display> Display for List<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let last = self.0.len().saturating_sub(1);
        for (i, item) in self.0.iter().enumerate() {
            let separator = match i {
                0 => "",
                n if n == last => " and ",
                _ => ", ",
            };
            write!(f, "{separator}{item}")?;
        }
        Ok(())
    }
}

/// Wall-clock `HH:MM:SS` (UTC), stamped on dev-server log lines.
pub(super) struct Clock;

impl Display for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let t = OffsetDateTime::now_utc();
        write!(f, "{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second())
    }
}

const DOT: &str = " · ";

/// The band a measured width is clamped into, and the width a layout stands in
/// with outside a terminal.
const MIN_WIDTH: usize = 40;
const MAX_WIDTH: usize = 200;
const NO_TERMINAL_WIDTH: usize = 100;

/// The usable width of one stream in columns, which is what every layout here
/// breaks against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Width(pub usize);

impl Width {
    /// The width of the payload stream, which is where a generated document is
    /// written.
    pub fn stdout() -> Self {
        Self::of(&console::Term::stdout())
    }

    /// The width of the message stream, which is where every progress and log
    /// line is written.
    pub fn stderr() -> Self {
        Self::of(&console::Term::stderr())
    }

    fn of(term: &console::Term) -> Self {
        Self(term.size_checked().map_or(NO_TERMINAL_WIDTH, |(_, cols)| {
            usize::from(cols).clamp(MIN_WIDTH, MAX_WIDTH)
        }))
    }
}

/// Greedy line breaking: the one place a list or a paragraph is laid out to a
/// terminal.
struct Fill;

impl Fill {
    /// Join `items` with `sep`, breaking before an item that would pass
    /// `width`, with every line after the first indented to `indent` columns.
    fn lay<'a>(
        items: impl Iterator<Item = &'a str>,
        sep: &str,
        indent: usize,
        width: usize,
    ) -> String {
        let gap = console::measure_text_width(sep);
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        let mut col = indent;
        for item in items {
            let w = console::measure_text_width(item);
            if line.is_empty() {
                line.push_str(item);
                col = indent + w;
            } else if col + gap + w <= width {
                line.push_str(sep);
                line.push_str(item);
                col += gap + w;
            } else {
                lines.push(std::mem::take(&mut line));
                line.push_str(item);
                col = indent + w;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines.join(&format!("\n{}", " ".repeat(indent)))
    }
}

/// A `·`-separated list laid out to the terminal, every line after the first
/// indented to `indent` columns.
pub struct Wrap<'a> {
    items: &'a [String],
    indent: usize,
    width: usize,
}

impl<'a> Wrap<'a> {
    /// Lay `items` out under `indent`, to the width of the stream they are
    /// written to.
    pub fn new(items: &'a [String], indent: usize) -> Self {
        Self {
            items,
            indent,
            width: Width::stderr().0,
        }
    }

    /// A layout at a stated width, so a test describes its own terminal.
    #[cfg(test)]
    fn at(items: &'a [String], indent: usize, width: usize) -> Self {
        Self {
            items,
            indent,
            width,
        }
    }
}

impl Display for Wrap<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let laid = Fill::lay(
            self.items.iter().map(String::as_str),
            DOT,
            self.indent,
            self.width,
        );
        write!(f, "{laid}")
    }
}

/// Prose laid out to `width` columns, every line after the first indented to
/// `indent`. Whitespace in the text is not preserved: it is what separates one
/// word from the next.
pub struct Prose<'a> {
    text: &'a str,
    indent: usize,
    width: usize,
}

impl<'a> Prose<'a> {
    /// Lay `text` out under `indent`, to a stated width.
    pub fn at(text: &'a str, indent: usize, width: usize) -> Self {
        Self {
            text,
            indent,
            width,
        }
    }

    /// The column the laid-out text's last line finishes at.
    pub fn end(&self) -> usize {
        let laid = self.to_string();
        let tail = laid.rsplit('\n').next().unwrap_or_default();
        console::measure_text_width(tail) + if laid.contains('\n') { 0 } else { self.indent }
    }
}

impl Display for Prose<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let laid = Fill::lay(self.text.split_whitespace(), " ", self.indent, self.width);
        write!(f, "{laid}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[&str]) -> Vec<String> {
        list.iter().copied().map(str::to_owned).collect()
    }

    fn wrap(list: &[&str], indent: usize, width: usize) -> String {
        Wrap::at(&items(list), indent, width).to_string()
    }

    #[test]
    fn wrap_keeps_a_short_list_on_one_line() {
        assert_eq!(wrap(&["a", "b", "c"], 2, 80), "a · b · c");
    }

    #[test]
    fn wrap_breaks_at_width_and_aligns_continuations() {
        assert_eq!(
            wrap(&["aaa", "bbb", "ccc", "ddd"], 2, 14),
            "aaa · bbb\n  ccc · ddd"
        );
    }

    #[test]
    fn wrap_gives_an_overlong_item_its_own_line() {
        assert_eq!(
            wrap(&["short", "a-very-long-single-item"], 0, 10),
            "short\na-very-long-single-item"
        );
    }

    #[test]
    fn wrap_counts_the_multibyte_separator_by_columns() {
        assert_eq!(wrap(&["aaaa", "bbbb"], 0, 11), "aaaa · bbbb");
    }

    #[test]
    fn a_measured_width_stays_in_the_band() {
        for width in [Width::stdout().0, Width::stderr().0] {
            assert!(
                width == NO_TERMINAL_WIDTH || (MIN_WIDTH..=MAX_WIDTH).contains(&width),
                "{width}"
            );
        }
    }

    #[test]
    fn prose_breaks_at_width_and_hangs_the_rest_at_the_indent() {
        assert_eq!(
            Prose::at("one two three four", 4, 16).to_string(),
            "one two\n    three four"
        );
    }

    #[test]
    fn prose_collapses_the_whitespace_it_breaks_on() {
        assert_eq!(Prose::at("one\n  two", 0, 40).to_string(), "one two");
    }

    #[test]
    fn durations_pick_the_tightest_unit() {
        assert_eq!(Dur(Duration::from_micros(840)).to_string(), "840µs");
        assert_eq!(Dur(Duration::from_millis(132)).to_string(), "132ms");
        assert_eq!(Dur(Duration::from_millis(1240)).to_string(), "1.24s");
        assert_eq!(Dur(Duration::from_secs(75)).to_string(), "1m 15s");
    }

    #[test]
    fn durations_parse_their_units() {
        let parse = |text| Dur::parse(text).map(|d| d.0);
        assert_eq!(parse("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse("10s"), Some(Duration::from_secs(10)));
        assert_eq!(parse("7d"), Some(Duration::from_hours(7 * 24)));
        assert_eq!(parse("1.5h"), Some(Duration::from_mins(90)));
        assert_eq!(parse("250ms"), Some(Duration::from_millis(250)));
        assert_eq!(parse(" 5 M "), Some(Duration::from_mins(5)));
        assert_eq!(parse("0"), Some(Duration::ZERO));
    }

    #[test]
    fn durations_refuse_what_is_not_one() {
        assert_eq!(Dur::parse("soon").map(|d| d.0), None);
        assert_eq!(Dur::parse("-1s").map(|d| d.0), None);
        assert_eq!(Dur::parse("").map(|d| d.0), None);
        assert_eq!(Dur::parse("3w").map(|d| d.0), None);
    }

    #[test]
    fn counts_pluralize() {
        assert_eq!(Count::pages(1).to_string(), "1 page");
        assert_eq!(Count::pages(3).to_string(), "3 pages");
    }

    #[test]
    fn bytes_scale_binary() {
        assert_eq!(Bytes(512).to_string(), "512 B");
        assert_eq!(Bytes(1024).to_string(), "1.0 KiB");
        assert_eq!(Bytes(1_468_006).to_string(), "1.4 MiB");
    }

    #[test]
    fn a_printed_size_parses_back_to_itself() {
        for size in [Bytes(0), Bytes(512), Bytes(51_200), Bytes(2 << 20)] {
            assert_eq!(Bytes::parse(&size.to_string()), Some(size), "{size}");
        }
    }

    #[test]
    fn sizes_parse_with_or_without_a_unit() {
        assert_eq!(Bytes::parse("0"), Some(Bytes(0)));
        assert_eq!(Bytes::parse("500"), Some(Bytes(500)));
        assert_eq!(Bytes::parse("50kB"), Some(Bytes(51_200)));
        assert_eq!(Bytes::parse("50 KiB"), Some(Bytes(51_200)));
        assert_eq!(Bytes::parse("1.5mb"), Some(Bytes(1_572_864)));
    }

    #[test]
    fn a_size_that_is_not_a_size_is_rejected() {
        assert_eq!(Bytes::parse("big"), None);
        assert_eq!(Bytes::parse("50 furlongs"), None);
        assert_eq!(Bytes::parse("-1"), None);
        assert_eq!(Bytes::parse(""), None);
    }
}
