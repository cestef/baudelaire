//! Terminal output: one shared, thread-safe [`Ui`] behind every line the CLI
//! prints.
//!
//! Layers:
//! - **Reporting** ([`Ui`]) — banners, results, per-page progress, dev-server
//!   event lines. Human-facing, on stderr, level-gated.
//! - **Warnings** ([`Ui::warn`]) — full [`miette::Diagnostic`]s with codes,
//!   spans, and help, collected during a run and rendered together by
//!   [`Ui::flush`], so a build's noise never interleaves with its progress.
//! - **Debug logs** ([`trace`]) — `tracing` events for `-v`/`-vv`/`RUST_LOG`,
//!   strictly diagnostic.
//!
//! Everything goes to stderr (stdout stays reserved for data), through
//! `anstream` so color strips on pipes and under `NO_COLOR`. Output is
//! best-effort by design: a failed terminal write never fails a build, so every
//! method here is infallible.

mod fmt;
mod progress;
pub mod trace;

use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use anstream::AutoStream;
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, Severity};
use owo_colors::OwoColorize;
use parking_lot::Mutex;

pub use fmt::{Bytes, Count, Dur, Paths};
pub use progress::Progress;

/// Output verbosity level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Nothing but collected warnings — used internally while the dev server
    /// rebuilds, so a rebuild reads as one concise log line, not a full build
    /// block.
    Silent,
    /// Only warnings and the final result.
    Quiet,
    /// Default: banner, results, warnings.
    #[default]
    Default,
    /// Verbose: + per-page progress and detail. Debug *logs* are separate —
    /// `-v` also enables them, via [`trace`].
    Verbose,
}

/// A started stopwatch, for the operation summaries. Explicit — the caller
/// times what it means to time, rather than the writer guessing.
pub struct Timer(Instant);

impl Timer {
    pub fn start() -> Self {
        Self(Instant::now())
    }

    pub fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

/// A collected diagnostic and how it counts: warnings tally into the build
/// summary, advice is informational only.
struct Note(Box<dyn Diagnostic + Send + Sync>);

impl Note {
    fn is_warning(&self) -> bool {
        // Diagnostics default to `Error` severity when unset; anything at
        // warning-or-worse counts against the build (advice does not).
        self.0.severity().unwrap_or(Severity::Error) >= Severity::Warning
    }
}

struct State {
    out: AutoStream<std::io::Stderr>,
    level: Level,
    notes: Vec<Note>,
    warned: usize,
}

/// The shared terminal reporter. All methods take `&self` (state sits behind a
/// mutex), so one `Ui` threads freely through rayon workers and the dev
/// server's request thread.
pub struct Ui {
    state: Mutex<State>,
    tty: bool,
}

impl Ui {
    pub fn new(level: Level) -> Self {
        Self {
            state: Mutex::new(State {
                out: AutoStream::auto(std::io::stderr()),
                level,
                notes: Vec::new(),
                warned: 0,
            }),
            tty: std::io::stderr().is_terminal(),
        }
    }

    /// The current verbosity, so callers can quiet a sub-operation and restore
    /// it (the dev server silences the full build block during a rebuild).
    pub fn level(&self) -> Level {
        self.state.lock().level
    }

    pub fn set_level(&self, level: Level) {
        self.state.lock().level = level;
    }

    /// Total warnings collected so far. Diffed across a build for its summary.
    pub fn warnings(&self) -> usize {
        self.state.lock().warned
    }

    /// The command banner: `baudelaire v0.1.0  building my-site`. Printed once
    /// per invocation, before any work.
    pub fn banner(&self, action: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        let _ = writeln!(
            s.out,
            "\n  {} {}  {}\n",
            "baudelaire".magenta().bold(),
            concat!("v", env!("CARGO_PKG_VERSION")).dimmed(),
            action
        );
    }

    /// A stage heading set off by a blank line: `◆ standard.site — 24 documents`.
    pub fn section(&self, msg: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        let _ = writeln!(s.out, "\n  {} {}", "◆".cyan().bold(), msg.bold());
    }

    /// A result line: `✓ built 24 pages .. in 132ms`. Shown even at `--quiet`
    /// (it is the final result); only [`Level::Silent`] suppresses it.
    pub fn done(&self, msg: impl Display) {
        self.done_inner(msg, true);
    }

    /// like [`done`](Self::done) but without the leading indent (perfectionism alignment issues)
    pub fn done_plain(&self, msg: impl Display) {
        self.done_inner(msg, false);
    }

    fn done_inner(&self, msg: impl Display, indent: bool) {
        let mut s = self.state.lock();
        if s.level < Level::Quiet {
            return;
        }
        if indent {
            let _ = writeln!(s.out, "  {} {}", "✓".green().bold(), msg);
        } else {
            let _ = writeln!(s.out, "{} {}", "✓".green().bold(), msg);
        }
    }

    /// Muted secondary detail, indented under the current stage (default+).
    pub fn detail(&self, msg: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        let _ = writeln!(s.out, "    {}", msg.dimmed());
    }

    /// An indented sub-item beneath a primary line: `↳ detail`.
    pub fn item(&self, msg: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        let _ = writeln!(s.out, "    {} {}", "↳".dimmed(), msg);
    }

    /// A vite-style pointer line: `➜ local  http://..`. Labels align across
    /// consecutive arrows (padded to the widest expected label).
    pub fn arrow(&self, label: &str, value: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        // pad before styling — a width applied to the styled value would count
        // its escape codes and misalign the column.
        let _ = writeln!(
            s.out,
            "  {} {} {}",
            "➜".green().bold(),
            format!("{label:<9}").bold(),
            value
        );
    }

    /// A blank line, for vertical grouping. Suppressed when quiet.
    pub fn blank(&self) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        let _ = writeln!(s.out);
    }

    /// Per-page progress (verbose+).
    pub fn page(&self, path: impl Display, status: PageStatus) {
        let mut s = self.state.lock();
        if s.level < Level::Verbose {
            return;
        }
        let _ = writeln!(
            s.out,
            "    {} {} {}",
            status.icon(),
            Paths(&path.to_string()),
            status.label().dimmed()
        );
    }

    /// A skipped item and why (verbose+).
    pub fn skip(&self, path: impl Display, reason: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Verbose {
            return;
        }
        let _ = writeln!(
            s.out,
            "    {} {} {}",
            "·".dimmed(),
            Paths(&path.to_string()),
            reason.dimmed()
        );
    }

    /// Collect a warning: a full diagnostic with code, spans, and help,
    /// rendered by the next [`Ui::flush`] and counted in the build summary.
    pub fn warn(&self, warning: impl Diagnostic + Send + Sync + 'static) {
        let mut s = self.state.lock();
        let note = Note(Box::new(warning));
        s.warned += usize::from(note.is_warning());
        s.notes.push(note);
    }

    /// Collect an informational note (severity `Advice`): rendered with the
    /// warnings but never counted against the build.
    pub fn advice(&self, advice: impl Diagnostic + Send + Sync + 'static) {
        self.warn(advice);
    }

    /// Render everything collected since the last flush, miette-formatted and
    /// indented into the report column. Shown at every level — a warning
    /// survives `--quiet` and the dev server's silent rebuilds. Identical
    /// renders collapse into one block with a repeat count, so the same
    /// missing font across fifty pages reads as one warning, not fifty.
    pub fn flush(&self) {
        let mut s = self.state.lock();
        let notes = std::mem::take(&mut s.notes);
        if notes.is_empty() {
            return;
        }
        let handler = self.handler();
        let mut seen: Vec<(String, usize)> = Vec::new();
        for note in &notes {
            let mut text = String::new();
            if handler.render_report(&mut text, &*note.0).is_err() {
                text = note.0.to_string();
            }
            match seen.iter_mut().find(|(t, _)| *t == text) {
                Some((_, n)) => *n += 1,
                None => seen.push((text, 1)),
            }
        }
        // clear any pending transient status line so the first block starts clean.
        if self.tty {
            let _ = write!(s.out, "\r\x1b[2K");
        }
        for (text, count) in &seen {
            let _ = writeln!(s.out);
            for line in text.lines() {
                let _ = writeln!(s.out, "  {line}");
            }
            if *count > 1 {
                let _ = writeln!(s.out, "  {}", format!("(repeated {count} times)").dimmed());
            }
        }
        let _ = writeln!(s.out);
    }

    /// The renderer for collected diagnostics, sized to the terminal. Colors
    /// are always emitted — the `anstream` writer strips them on pipes and
    /// under `NO_COLOR`, same as every other line.
    fn handler(&self) -> GraphicalReportHandler {
        let width = console::Term::stderr()
            .size_checked()
            .map(|(_, cols)| (cols as usize).saturating_sub(4).clamp(60, 120))
            .unwrap_or(96);
        GraphicalReportHandler::new_themed(GraphicalTheme::unicode()).with_width(width)
    }

    /// A transient, in-place status line (no newline), overwritten by the next
    /// output. Used by the dev server while a rebuild is running.
    pub fn status(&self, msg: impl Display) {
        let mut s = self.state.lock();
        // A transient overwrite only makes sense on a terminal; on a pipe it
        // would leave a stranded line (and raw cursor escapes) in the log.
        if s.level < Level::Default || !self.tty {
            return;
        }
        let _ = write!(s.out, "\r\x1b[2K  {} {}", "⟳".cyan(), msg.dimmed());
        let _ = s.out.flush();
    }

    /// A dev-server event line: wall clock, change glyph, the file that
    /// triggered the rebuild, and what it cost. Clears any pending
    /// [`Ui::status`] line first, so rebuilds read as a tidy vite-style log.
    pub fn event(&self, path: impl Display, pages: usize, elapsed: Duration) {
        let mut s = self.state.lock();
        let clear = if self.tty { "\r\x1b[2K" } else { "" };
        let _ = writeln!(
            s.out,
            "{}  {}  {} {}  {} {}",
            clear,
            fmt::clock().dimmed(),
            "~".green(),
            Paths(&path.to_string()),
            Count::pages(pages).dimmed(),
            Dur(elapsed).dimmed()
        );
    }

    /// A dev-server request that missed (verbose+): `12:31:02  404 /favicon.ico`.
    pub fn request(&self, code: u16, url: &str) {
        let mut s = self.state.lock();
        if s.level < Level::Verbose {
            return;
        }
        let _ = writeln!(
            s.out,
            "  {}  {} {}",
            fmt::clock().dimmed(),
            code.yellow(),
            url.dimmed()
        );
    }

    /// A progress bar labeled `verb` over `len` items — visible only on a
    /// terminal at the default level (verbose prints per-page lines instead,
    /// quiet prints nothing).
    pub fn progress(&self, verb: &'static str, len: usize) -> Progress {
        if self.tty && self.level() == Level::Default && len > 0 {
            Progress::bar(verb, len as u64)
        } else {
            Progress::hidden()
        }
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
