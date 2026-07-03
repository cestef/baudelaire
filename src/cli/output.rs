use std::fmt::Display;
use std::io::Write;
use std::time::{Duration, Instant};

use anstream::AutoStream;
use owo_colors::OwoColorize;

/// Output verbosity level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Only errors and final results.
    Quiet,
    /// Default: milestones, success, warnings.
    #[default]
    Default,
    /// Verbose: + per-page progress, info.
    Verbose,
    /// Trace: + everything.
    Trace,
}

/// Colored CLI report writer.
pub struct Report {
    out: AutoStream<std::io::Stdout>,
    level: Level,
    started: Option<Instant>,
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
        }
    }

    pub fn set_level(&mut self, level: Level) {
        self.level = level;
    }

    /// Begin a timed operation. Duration shown in [`Report::done`].
    pub fn start(&mut self) {
        self.started = Some(Instant::now());
    }

    /// Elapsed time since [`Report::start`], if any.
    fn elapsed(&self) -> Option<Duration> {
        self.started.map(|t| t.elapsed())
    }

    /// Major stage marker: `◆ building - Site (5 pages)`.
    pub fn milestone(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level == Level::Quiet {
            return Ok(());
        }
        writeln!(self.out, "\n {} {}", "◆".cyan().bold(), detail.bold())
    }

    /// Success: `✓ built 5 pages in 12ms`.
    pub fn done(&mut self, detail: impl Display) -> std::io::Result<()> {
        let time = self.elapsed();
        match time {
            Some(d) => writeln!(
                self.out,
                " {} {} {}",
                "✓".green().bold(),
                detail,
                fmt_dur(d).dimmed()
            ),
            None => writeln!(self.out, " {} {}", "✓".green().bold(), detail),
        }
    }
    /// Plain success without timing.
    pub fn success(&mut self, detail: impl Display) -> std::io::Result<()> {
        writeln!(self.out, " {} {}", "✓".green().bold(), detail)
    }

    /// Per-page progress (verbose+).
    pub fn page(&mut self, path: impl Display, status: PageStatus) -> std::io::Result<()> {
        if self.level < Level::Verbose {
            return Ok(());
        }
        writeln!(
            self.out,
            "   {} {} {}",
            status.icon(),
            path.dimmed(),
            status.label().dimmed()
        )
    }

    /// Info line (verbose+).
    pub fn info(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level < Level::Verbose {
            return Ok(());
        }
        writeln!(self.out, " {} {}", "◇".blue(), detail.dimmed())
    }

    /// Warning (always shown), shown as a badge so it stands out in the flow.
    pub fn warn(&mut self, detail: impl Display) -> std::io::Result<()> {
        writeln!(self.out, " {} {}", " warning ".black().on_yellow().bold(), detail)
    }

    /// An indented sub-item beneath a milestone or warning: `↳ detail`. Aligns
    /// grouped detail (broken links, removed paths) under its heading.
    pub fn item(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level == Level::Quiet {
            return Ok(());
        }
        writeln!(self.out, "   {} {}", "↳".dimmed(), detail)
    }

    /// Muted secondary detail (default+).
    pub fn muted(&mut self, detail: impl Display) -> std::io::Result<()> {
        if self.level == Level::Quiet {
            return Ok(());
        }
        writeln!(self.out, "  {}", detail.dimmed())
    }

    /// Skipped page (verbose+).
    pub fn skipped(&mut self, path: impl Display, reason: impl Display) -> std::io::Result<()> {
        if self.level < Level::Verbose {
            return Ok(());
        }
        writeln!(
            self.out,
            "   {} {} {}",
            "·".dimmed(),
            path.dimmed(),
            reason.dimmed()
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

fn fmt_dur(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("({ms}ms)")
    } else {
        format!("({:.2}s)", d.as_secs_f64())
    }
}
