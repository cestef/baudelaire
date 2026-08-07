//! Display helpers: the single source of count, size, duration, path, and
//! wall-clock formatting across the CLI.

use std::fmt::Display;
use std::time::Duration;

use owo_colors::OwoColorize;
use time::OffsetDateTime;

/// Displays a count with a pluralized noun: `1 page` / `3 pages`. The single
/// source of count-noun phrasing, shared by the engine summary and the CLI.
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

    /// A count whose noun is not fixed at the call site: the sidecar registry
    /// names its own kinds (`"card"`), and the summary counts whatever it
    /// registered. Pluralization still happens here, so a registry cannot
    /// invent its own phrasing.
    pub fn of(n: usize, noun: &'static str) -> Self {
        Self { n, noun }
    }

    pub fn warnings(n: usize) -> Self {
        Self { n, noun: "warning" }
    }

    /// The noun with its plural `s`, so styling can treat number and label
    /// separately.
    fn label(&self) -> String {
        format!("{}{}", self.noun, if self.n == 1 { "" } else { "s" })
    }

    /// Summary styling: the number in bold, the label dimmed, so quantities
    /// pop and their nouns recede.
    pub fn styled(&self) -> StyledCount<'_> {
        StyledCount(self)
    }
}

impl Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.n, self.label())
    }
}

/// [`Count`] with the number bold and the label dimmed (see [`Count::styled`]).
pub struct StyledCount<'a>(&'a Count);

impl Display for StyledCount<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.0.n.bold(), self.0.label().dimmed())
    }
}

/// A byte count in binary units (`512 B`, `1.4 MiB`): 1024-based with one
/// decimal above the byte threshold. The single source of size formatting *and*
/// of reading a size back in, so a budget is authored in the units the summary
/// prints.
///
/// The units are labeled as what they are. The maths has always been 1024-based
/// and stays so, because every configured budget was authored against it; what
/// changed is the label, which said `KB` for 1024 bytes and so overstated a
/// budget by 2.4% at kilobytes and 4.9% at megabytes. `kB`/`MB` are still
/// *accepted* on the way in ([`Bytes::parse`]), where they have always meant the
/// same thing, so no config has to be rewritten.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bytes(pub u64);

impl Bytes {
    /// The units a size may be written in, largest first so `MB` is matched
    /// before `B`. The `i` spellings are accepted for authors who want to be
    /// explicit; they mean the same thing, since these units are 1024-based.
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
    // The float math is the point: `1.5 MB` has to scale. The value is checked
    // finite and non-negative, and a product that no longer fits a `u64` is
    // rejected below, so the narrowing back rounds an in-range number rather
    // than truncating, wrapping, or dropping a sign. `u64::MAX as f64` rounds
    // up to 2^64, which is the bound being tested for.
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
        // Beyond this a `f64` no longer counts single bytes, and a budget that
        // large is a typo rather than an intent.
        (bytes <= u64::MAX as f64).then_some(Self(bytes as u64))
    }

    /// The scaled number and its unit, split so styling can treat them
    /// separately.
    // Sizes past 2^53 bytes lose their last digits here, and the scaled form
    // this produces shows one decimal anyway.
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

    /// Summary styling: the number in bold, the unit dimmed.
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

/// [`Bytes`] with the number bold and the unit dimmed (see [`Bytes::styled`]).
pub struct StyledBytes<'a>(&'a Bytes);

impl Display for StyledBytes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (value, unit) = self.0.parts();
        write!(f, "{} {}", value.bold(), unit.dimmed())
    }
}

/// A duration in the tightest sensible unit (`840µs`, `132ms`, `1.24s`): the
/// single source of elapsed-time formatting (hyperfine-style, no parentheses).
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
    ///
    /// The counterpart of [`Bytes::parse`], and deliberately *not* the inverse
    /// of this type's [`Display`]: that renders an elapsed time for a reader
    /// (`1m 15s`), which is a sentence rather than a value, while this reads the
    /// single value an author writes. Sharing the type is what keeps the crate
    /// to one duration and one place that knows what `d` means.
    // Same float math, and for the same reason, as `Bytes::parse`: `1.5h` has to
    // scale. Checked finite and non-negative, and a product past `u64` is
    // rejected, so the narrowing rounds an in-range number.
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
            // A bare number is seconds, so it scales like one.
            .unwrap_or((text.as_str(), 1_000));
        let value: f64 = number.parse().ok()?;
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        // Beyond this an `f64` no longer counts single milliseconds, and a
        // config asking to wait that long is a typo rather than an intent.
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

/// Renders a path with its directory portion dimmed and the final component (the
/// file, or a bare directory) in cyan, so the eye lands on what changed. The
/// single source of path styling across the CLI.
pub struct Paths<'a>(pub &'a str);

impl Display for Paths<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `/` is ASCII, so the split index is always a char boundary.
        match self.0.rfind('/') {
            Some(i) => {
                let (dir, file) = self.0.split_at(i + 1);
                write!(f, "{}{}", dir.dimmed(), file.cyan())
            }
            None => write!(f, "{}", self.0.cyan()),
        }
    }
}

/// Renders items as prose: `a`, `a and b`, `a, b and c`. For a message naming a
/// handful of things, so no caller joins a list by hand and they all read the
/// same way.
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

/// Wall-clock `HH:MM:SS` (UTC) stamped on dev-server log lines.
pub(super) fn clock() -> String {
    let t = OffsetDateTime::now_utc();
    format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second())
}

/// The `·`-separated list separator shared by multi-item lines (the watch list,
/// the build breakdown), so they wrap and read the same way.
const DOT: &str = " · ";

/// The band [`Wrap`] clamps a terminal's real width into: below 40 columns
/// wrapping stops helping, and past 200 a line is too long to scan back along.
/// Outside a terminal there is no width to read, so a conventional 100 stands
/// in.
const MIN_WIDTH: usize = 40;
const MAX_WIDTH: usize = 200;
const NO_TERMINAL_WIDTH: usize = 100;

/// A `·`-separated list laid out to the terminal: the watch list, the build
/// breakdown. Every line after the first is indented to `indent` columns, so
/// continuations align under the first item instead of running off-screen. A
/// single item wider than the budget takes its own line rather than being split.
///
/// The width is the type's own business, measured once when the layout is made:
/// a caller has a list and a column to hang it under, and nothing to say about
/// how wide the terminal is.
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
            width: Self::width(),
        }
    }

    /// The usable width in columns, clamped to a sane band and falling back
    /// when the size is unavailable (piped output, no tty).
    ///
    /// Measured on **stderr**, which is where every line laid out here is
    /// written. It used to measure `Term::stdout()`: a run whose stdout was
    /// redirected (`--json`, or a tee'd log) read no size at all and wrapped
    /// the dev server's banner to 100 columns on a terminal twice that wide,
    /// while its stderr was sitting on the real one.
    fn width() -> usize {
        console::Term::stderr()
            .size_checked()
            .map_or(NO_TERMINAL_WIDTH, |(_, cols)| {
                usize::from(cols).clamp(MIN_WIDTH, MAX_WIDTH)
            })
    }

    /// A layout at a stated width, so the tests below describe a terminal
    /// rather than depend on the one they happen to run under.
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
        // Measure display columns, not bytes: the separator's middle dot is
        // multi-byte, and an item may carry ANSI color (the build breakdown
        // does), both of which a byte or char count would get wrong.
        let sep = console::measure_text_width(DOT);
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        let mut col = self.indent;
        for item in self.items {
            let w = console::measure_text_width(item);
            if line.is_empty() {
                line.push_str(item);
                col = self.indent + w;
            } else if col + sep + w <= self.width {
                line.push_str(DOT);
                line.push_str(item);
                col += sep + w;
            } else {
                lines.push(std::mem::take(&mut line));
                line.push_str(item);
                col = self.indent + w;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        write!(
            f,
            "{}",
            lines.join(&format!("\n{}", " ".repeat(self.indent)))
        )
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
        // indent 2: "aaa · bbb" ends at col 11; " · ccc" would reach 17 > 14, so
        // ccc starts a new line padded to the indent.
        assert_eq!(
            wrap(&["aaa", "bbb", "ccc", "ddd"], 2, 14),
            "aaa · bbb\n  ccc · ddd"
        );
    }

    #[test]
    fn wrap_gives_an_overlong_item_its_own_line() {
        // A single item wider than the budget is not split; it just sits alone.
        assert_eq!(
            wrap(&["short", "a-very-long-single-item"], 0, 10),
            "short\na-very-long-single-item"
        );
    }

    #[test]
    fn wrap_counts_the_multibyte_separator_by_columns() {
        // ` · ` is 3 columns but 4 bytes; two 4-char items plus a separator is
        // 11 columns, which fits a width of 11 exactly.
        assert_eq!(wrap(&["aaaa", "bbbb"], 0, 11), "aaaa · bbbb");
    }

    /// The width comes off stderr, the stream every wrapped line is written to.
    /// Whatever the test terminal is, it stays inside the band.
    #[test]
    fn the_measured_width_stays_in_the_band() {
        let width = Wrap::width();
        assert!(
            width == NO_TERMINAL_WIDTH || (MIN_WIDTH..=MAX_WIDTH).contains(&width),
            "{width}"
        );
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
        // A bare number is seconds, which is what a timeout is written in.
        assert_eq!(parse("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse("10s"), Some(Duration::from_secs(10)));
        assert_eq!(parse("7d"), Some(Duration::from_hours(7 * 24)));
        assert_eq!(parse("1.5h"), Some(Duration::from_mins(90)));
        // `ms` is matched before the `s` inside it.
        assert_eq!(parse("250ms"), Some(Duration::from_millis(250)));
        assert_eq!(parse(" 5 M "), Some(Duration::from_mins(5)));
        assert_eq!(parse("0"), Some(Duration::ZERO));
    }

    #[test]
    fn durations_refuse_what_is_not_one() {
        assert_eq!(Dur::parse("soon").map(|d| d.0), None);
        assert_eq!(Dur::parse("-1s").map(|d| d.0), None);
        assert_eq!(Dur::parse("").map(|d| d.0), None);
        // A unit this crate does not know, rather than a number with a stray
        // letter that happens to end in one it does.
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

    /// A printed size reads back in as itself: the label names the same unit
    /// the scaling used, which is the whole point of relabeling them.
    #[test]
    fn a_printed_size_parses_back_to_itself() {
        for size in [Bytes(0), Bytes(512), Bytes(51_200), Bytes(2 << 20)] {
            assert_eq!(Bytes::parse(&size.to_string()), Some(size), "{size}");
        }
    }

    /// A size reads back in the units it prints in, plus the spellings an
    /// author is likely to reach for.
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
