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

    pub fn cards(n: usize) -> Self {
        Self { n, noun: "card" }
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

/// A byte count in binary units (`512 B`, `1.4 MB`): 1024-based with one
/// decimal above the byte threshold. The single source of size formatting.
pub struct Bytes(pub u64);

impl Bytes {
    /// The scaled number and its unit, split so styling can treat them
    /// separately.
    fn parts(&self) -> (String, &'static str) {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
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

impl Display for Dur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let d = self.0;
        if d < Duration::from_millis(1) {
            write!(f, "{}µs", d.as_micros())
        } else if d < Duration::from_secs(1) {
            write!(f, "{}ms", d.as_millis())
        } else if d < Duration::from_secs(60) {
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

/// Wall-clock `HH:MM:SS` (UTC) stamped on dev-server log lines.
pub(super) fn clock() -> String {
    let t = OffsetDateTime::now_utc();
    format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second())
}

/// The `·`-separated list separator shared by multi-item lines (the watch list,
/// the build breakdown), so they wrap and read the same way.
const DOT: &str = " · ";

/// The band [`term_width`] clamps a terminal's real width into: below 40
/// columns wrapping stops helping, and past 200 a line is too long to scan back
/// along. Outside a terminal there is no width to read, so a conventional 100
/// stands in.
const MIN_WIDTH: usize = 40;
const MAX_WIDTH: usize = 200;
const NO_TERMINAL_WIDTH: usize = 100;

/// The usable terminal width in columns, clamped to a sane band and falling back
/// when the size is unavailable (piped output, no tty). The single width source
/// for [`wrap`].
pub fn term_width() -> usize {
    console::Term::stdout()
        .size_checked()
        .map(|(_, cols)| (cols as usize).clamp(MIN_WIDTH, MAX_WIDTH))
        .unwrap_or(NO_TERMINAL_WIDTH)
}

/// Lay out `·`-separated `items` so no line exceeds `width`, with every line
/// after the first indented to `indent` columns, aligning continuations under
/// the first item. Returns the ready-to-print value (embedded newlines and all),
/// so a long watch list or build breakdown flows onto extra lines instead of
/// running off-screen. A single item wider than the budget still takes its own
/// line rather than being split.
pub fn wrap(items: &[String], indent: usize, width: usize) -> String {
    // Measure display columns, not bytes: the separator's middle dot is
    // multi-byte, and an item may carry ANSI color (the build breakdown does),
    // both of which a byte or char count would get wrong.
    let sep = console::measure_text_width(DOT);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut col = indent;
    for item in items {
        let w = console::measure_text_width(item);
        if line.is_empty() {
            line.push_str(item);
            col = indent + w;
        } else if col + sep + w <= width {
            line.push_str(DOT);
            line.push_str(item);
            col += sep + w;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn wrap_keeps_a_short_list_on_one_line() {
        assert_eq!(wrap(&items(&["a", "b", "c"]), 2, 80), "a · b · c");
    }

    #[test]
    fn wrap_breaks_at_width_and_aligns_continuations() {
        // indent 2: "aaa · bbb" ends at col 11; " · ccc" would reach 17 > 14, so
        // ccc starts a new line padded to the indent.
        assert_eq!(
            wrap(&items(&["aaa", "bbb", "ccc", "ddd"]), 2, 14),
            "aaa · bbb\n  ccc · ddd"
        );
    }

    #[test]
    fn wrap_gives_an_overlong_item_its_own_line() {
        // A single item wider than the budget is not split; it just sits alone.
        assert_eq!(
            wrap(&items(&["short", "a-very-long-single-item"]), 0, 10),
            "short\na-very-long-single-item"
        );
    }

    #[test]
    fn wrap_counts_the_multibyte_separator_by_columns() {
        // ` · ` is 3 columns but 4 bytes; two 4-char items plus a separator is
        // 11 columns, which fits a width of 11 exactly.
        assert_eq!(wrap(&items(&["aaaa", "bbbb"]), 0, 11), "aaaa · bbbb");
    }

    #[test]
    fn durations_pick_the_tightest_unit() {
        assert_eq!(Dur(Duration::from_micros(840)).to_string(), "840µs");
        assert_eq!(Dur(Duration::from_millis(132)).to_string(), "132ms");
        assert_eq!(Dur(Duration::from_millis(1240)).to_string(), "1.24s");
        assert_eq!(Dur(Duration::from_secs(75)).to_string(), "1m 15s");
    }

    #[test]
    fn counts_pluralize() {
        assert_eq!(Count::pages(1).to_string(), "1 page");
        assert_eq!(Count::pages(3).to_string(), "3 pages");
    }

    #[test]
    fn bytes_scale_binary() {
        assert_eq!(Bytes(512).to_string(), "512 B");
        assert_eq!(Bytes(1_468_006).to_string(), "1.4 MB");
    }
}
