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

#[cfg(test)]
mod tests {
    use super::*;

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
