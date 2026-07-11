use std::fmt::Display;
use std::io::Write;
use std::time::{Duration, Instant};

use anstream::AutoStream;
use owo_colors::OwoColorize;
use time::OffsetDateTime;

/// Output verbosity level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Nothing but warnings — used internally while the dev server rebuilds, so a
    /// rebuild reads as one concise log line, not a full build block.
    Silent,
    /// Only warnings and the final result.
    Quiet,
    /// Default: milestones, result, warnings.
    #[default]
    Default,
    /// Verbose: + per-page progress, info.
    Verbose,
    /// Trace: + everything.
    Trace,
}

/// Colored CLI report writer.
///
/// Indentation is uniform: primary lines (milestone, result, warning, info) sit
/// in a two-space column behind a glyph; sub-items (per-page, grouped detail) are
/// indented one level further. Nothing else varies the margin.
pub struct Report {
    out: AutoStream<std::io::Stdout>,
    level: Level,
    started: Option<Instant>,
    warnings: usize,
}

impl Report {
    pub fn stdout() -> Self {
        Self::with_level(Level::Default)
    }

    pub fn with_level(level: Level) -> Self {
        Self {
            out: AutoStream::auto(std::io::stdout()),
            level,
            started: None,
            warnings: 0,
        }
    }

    pub fn set_level(&mut self, level: Level) {
        self.level = level;
    }

    /// The current verbosity, so callers can quiet a sub-operation and restore it
    /// (the dev server silences the full build block during a rebuild).
    pub fn level(&self) -> Level {
        self.level
    }

    /// Total warnings emitted so far. Diffed across a build for its summary.
    pub fn warnings(&self) -> usize {
        self.warnings
    }

    /// Begin a timed operation. Duration shown in [`Report::done`].
    pub fn start(&mut self) {
        self.started = Some(Instant::now());
    }

    /// Elapsed time since [`Report::start`], if any.
    fn elapsed(&self) -> Option<Duration> {
        self.started.map(|t| t.elapsed())
    }

    /// Major stage marker: `◆ building Site (5 pages)`, set off by a blank line.
    pub fn milestone(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level <= Level::Quiet {
            return Ok(());
        }
        writeln!(self.out, "\n  {} {}", "◆".cyan().bold(), detail.bold())
    }

    /// Timed result: `✓ built 5 pages (12ms)`. Shown even at `--quiet` (it is the
    /// final result); only [`Level::Silent`] suppresses it.
    pub fn done(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level < Level::Quiet {
            return Ok(());
        }
        match self.elapsed() {
            Some(d) => writeln!(self.out, "  {} {} {}", "✓".green().bold(), detail, fmt_dur(d).dimmed()),
            None => writeln!(self.out, "  {} {}", "✓".green().bold(), detail),
        }
    }

    /// Plain success without timing.
    pub fn success(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level < Level::Quiet {
            return Ok(());
        }
        writeln!(self.out, "  {} {}", "✓".green().bold(), detail)
    }

    /// Per-page progress (verbose+).
    pub fn page(&mut self, path: impl Display, status: PageStatus) -> std::io::Result<()> {
        if self.level < Level::Verbose {
            return Ok(());
        }
        writeln!(self.out, "    {} {} {}", status.icon(), Paths(&path.to_string()), status.label().dimmed())
    }

    /// Info line (verbose+).
    pub fn info(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level < Level::Verbose {
            return Ok(());
        }
        writeln!(self.out, "  {} {}", "◇".blue(), detail.dimmed())
    }

    /// Warning (always shown), and counted for the build summary.
    pub fn warn(&mut self, detail: impl Display) -> std::io::Result<()> {
        self.warnings += 1;
        writeln!(self.out, "  {} {}", "⚠".yellow().bold(), detail)
    }

    /// An indented sub-item beneath a primary line: `↳ detail`. Aligns grouped
    /// detail (broken links, a build's summary, removed paths) under its heading.
    pub fn item(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level <= Level::Quiet {
            return Ok(());
        }
        writeln!(self.out, "    {} {}", "↳".dimmed(), detail)
    }

    /// A blank line, for vertical grouping. Suppressed when quiet.
    pub fn blank(&mut self) -> std::io::Result<()> {
        if self.level <= Level::Quiet {
            return Ok(());
        }
        writeln!(self.out)
    }

    /// Muted secondary detail, in the sub-item column (default+).
    pub fn muted(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level <= Level::Quiet {
            return Ok(());
        }
        writeln!(self.out, "    {}", detail.dimmed())
    }

    /// Skipped page (verbose+).
    pub fn skipped(&mut self, path: impl Display, reason: impl Display) -> std::io::Result<()> {
        if self.level < Level::Verbose {
            return Ok(());
        }
        writeln!(self.out, "    {} {} {}", "·".dimmed(), Paths(&path.to_string()), reason.dimmed())
    }

    /// A transient, in-place status line (no newline), overwritten by the next
    /// output. Used by the dev server while a rebuild is running.
    pub fn status(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level == Level::Quiet {
            return Ok(());
        }
        write!(self.out, "\r\x1b[2K  {} {}", "⟳".cyan(), detail.dimmed())?;
        self.out.flush()
    }

    /// A dev-server rebuild log line: a wall-clock timestamp, a change glyph, the
    /// file that triggered it, and how many pages were rebuilt and how long it
    /// took. Clears any pending [`Report::status`] line first, so rebuilds read as
    /// a tidy Vite-style log rather than stacked build blocks.
    pub fn event(&mut self, path: impl Display, pages: usize) -> std::io::Result<()> {
        let elapsed = self.elapsed().map(fmt_dur).unwrap_or_default();
        writeln!(
            self.out,
            "\r\x1b[2K  {}  {} {}  {} {}",
            now().dimmed(),
            "~".green(),
            Paths(&path.to_string()),
            Count::pages(pages).dimmed(),
            elapsed.dimmed()
        )
    }
}

/// Per-page build status for progress reporting.
#[derive(Debug, Clone, Copy)]
pub enum PageStatus {
    Built,
    Cached,
    Failed,
}

impl PageStatus {
    /// The status glyph, already colored for its meaning.
    fn icon(self) -> String {
        match self {
            Self::Built => "✓".green().to_string(),
            Self::Cached => "→".cyan().to_string(),
            Self::Failed => "✗".red().to_string(),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Built => "built",
            Self::Cached => "cached",
            Self::Failed => "failed",
        }
    }
}

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

    pub fn redirects(n: usize) -> Self {
        Self { n, noun: "redirect" }
    }

    pub fn assets(n: usize) -> Self {
        Self { n, noun: "asset" }
    }

    pub fn files(n: usize) -> Self {
        Self { n, noun: "file" }
    }

    pub fn warnings(n: usize) -> Self {
        Self { n, noun: "warning" }
    }

    pub fn links(n: usize) -> Self {
        Self { n, noun: "broken internal link" }
    }
}

impl Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}{}", self.n, self.noun, if self.n == 1 { "" } else { "s" })
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
fn now() -> String {
    let t = OffsetDateTime::now_utc();
    format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second())
}

fn fmt_dur(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("({ms}ms)")
    } else {
        format!("({:.2}s)", d.as_secs_f64())
    }
}
