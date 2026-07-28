//! Terminal output: one shared, thread-safe [`Ui`] behind every line the CLI
//! prints.
//!
//! Layers:
//! - **Reporting** ([`Ui`]): banners, results, per-page progress, dev-server
//!   event lines. Human-facing, on stderr, level-gated.
//! - **Warnings** ([`Ui::warn`]): full [`miette::Diagnostic`]s with codes,
//!   spans, and help, collected during a run and rendered together by
//!   [`Ui::flush`], so a build's noise never interleaves with its progress.
//! - **Debug logs** ([`trace`]): `tracing` events for `-v`/`-vv`/`RUST_LOG`,
//!   strictly diagnostic.
//!
//! What a line is made of lives beside it: `marker` is the one table of status
//! glyphs and their colours, `fmt` the count, size, duration and path adapters
//! every line formats through, and `progress` the compile bar.
//!
//! Everything goes to stderr (stdout stays reserved for data), through
//! `anstream` so color strips on pipes and under `NO_COLOR`. Output is
//! best-effort by design: a failed terminal write never fails a build, so every
//! method here is infallible.

mod fmt;
mod marker;
mod progress;
pub mod trace;

use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use anstream::AutoStream;
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, Severity};
use owo_colors::OwoColorize;
use parking_lot::Mutex;

pub use fmt::{Bytes, Count, Dur, Paths, term_width, wrap};
pub use marker::{Marker, PageStatus};
pub use progress::Progress;

/// Return the cursor to column 0 and erase the line: what makes a transient
/// status line transient. Written raw because it is cursor control rather than
/// styling, so `anstream` has nothing to strip; every use is guarded by a tty
/// check, since on a pipe it would strand the escape in the log.
const CLEAR_LINE: &str = "\r\x1b[2K";

/// The width `➜` labels are padded to, so consecutive arrows line their values
/// up. Sized to the longest label in use (`watching`).
const ARROW_LABEL: usize = 9;

/// The column an arrow's value starts at: the two-space indent, the glyph, a
/// space, the padded label, a space. A caller laying out a multi-line value
/// aligns its continuations here.
pub const ARROW_VALUE_COLUMN: usize = 2 + 1 + 1 + ARROW_LABEL + 1;

/// The band the diagnostic renderer's width is clamped into, and what it uses
/// when the terminal size is unavailable (piped output). Narrower than 60
/// columns shreds the help text; past 120 the prose is hard to track back.
const REPORT_MIN_WIDTH: usize = 60;
const REPORT_MAX_WIDTH: usize = 120;
const REPORT_NO_TERMINAL_WIDTH: usize = 96;

/// Columns [`Ui::flush`] spends indenting each rendered line into the report
/// column, counted twice so the box keeps the same margin on its right.
const REPORT_MARGIN: usize = 4;

/// Output verbosity level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Nothing but collected warnings: used internally while the dev server
    /// rebuilds, so a rebuild reads as one concise log line, not a full build
    /// block.
    Silent,
    /// Only warnings and the final result.
    Quiet,
    /// Default: banner, results, warnings.
    #[default]
    Default,
    /// Verbose: + per-page progress and detail. Debug *logs* are separate:
    /// `-v` also enables them, via [`trace`].
    Verbose,
}

/// A started stopwatch, for the operation summaries. Explicit: the caller
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

    /// A stage heading set off by a blank line: `◆ standard.site - 24 documents`.
    pub fn section(&self, msg: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        let _ = writeln!(s.out, "\n  {} {}", Marker::Section, msg.bold());
    }

    /// A result line: `✓ built 24 pages .. in 132ms`. Shown even at `--quiet`
    /// (it is the final result); only [`Level::Silent`] suppresses it.
    pub fn done(&self, msg: impl Display) {
        self.done_inner(msg, true);
    }

    /// Like [`done`](Self::done) but flush left, for a result that stands on its
    /// own rather than closing an indented stage.
    pub fn done_plain(&self, msg: impl Display) {
        self.done_inner(msg, false);
    }

    fn done_inner(&self, msg: impl Display, indent: bool) {
        let mut s = self.state.lock();
        if s.level < Level::Quiet {
            return;
        }
        if indent {
            let _ = writeln!(s.out, "  {} {}", Marker::Done, msg);
        } else {
            let _ = writeln!(s.out, "{} {}", Marker::Done, msg);
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
        let _ = writeln!(s.out, "    {} {}", Marker::Item, msg);
    }

    /// Rows hung off the preceding result as a dimmed tree: each row gets a
    /// `├─` connector, the last a rounded `╰─`, aligned under the result glyph.
    pub fn tree(&self, rows: &[String]) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        let last = rows.len().saturating_sub(1);
        for (i, row) in rows.iter().enumerate() {
            let connector = if i == last {
                Marker::End
            } else {
                Marker::Branch
            };
            let _ = writeln!(s.out, "  {connector} {row}");
        }
    }

    /// A vite-style pointer line: `➜ local  http://..`. Labels align across
    /// consecutive arrows (padded to the widest expected label).
    pub fn arrow(&self, label: &str, value: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Default {
            return;
        }
        // pad before styling: a width applied to the styled value would count
        // its escape codes and misalign the column.
        let _ = writeln!(
            s.out,
            "  {} {} {}",
            Marker::Pointer,
            format!("{label:<ARROW_LABEL$}").bold(),
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
            status.marker(),
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
            Marker::Skipped,
            Paths(&path.to_string()),
            reason.dimmed()
        );
    }

    /// Collect a warning: a full diagnostic with code, spans, and help,
    /// rendered by the next [`Ui::flush`] and counted in the build summary.
    pub fn warn(&self, warning: impl Diagnostic + Send + Sync + 'static) {
        self.report(Box::new(warning));
    }

    /// Collect an already-boxed warning, for callers reached through a trait
    /// object that cannot name the concrete type (see
    /// [`crate::engine::process::Emit::warn`]).
    pub fn report(&self, warning: Box<dyn Diagnostic + Send + Sync>) {
        let mut s = self.state.lock();
        let note = Note(warning);
        s.warned += usize::from(note.is_warning());
        s.notes.push(note);
    }

    /// Collect an informational note (severity `Advice`): rendered with the
    /// warnings but never counted against the build.
    pub fn advice(&self, advice: impl Diagnostic + Send + Sync + 'static) {
        self.warn(advice);
    }

    /// Render everything collected since the last flush, miette-formatted and
    /// indented into the report column. Shown at every level: a warning
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
            let _ = write!(s.out, "{CLEAR_LINE}");
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
    /// are always emitted: the `anstream` writer strips them on pipes and
    /// under `NO_COLOR`, same as every other line.
    fn handler(&self) -> GraphicalReportHandler {
        let width = console::Term::stderr()
            .size_checked()
            .map(|(_, cols)| {
                (cols as usize)
                    .saturating_sub(REPORT_MARGIN)
                    .clamp(REPORT_MIN_WIDTH, REPORT_MAX_WIDTH)
            })
            .unwrap_or(REPORT_NO_TERMINAL_WIDTH);
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
        let _ = write!(s.out, "{CLEAR_LINE}  {} {}", Marker::Working, msg.dimmed());
        let _ = s.out.flush();
    }

    /// A dev-server event line: wall clock, change glyph, the file that
    /// triggered the rebuild, and what it cost. Clears any pending
    /// [`Ui::status`] line first, so rebuilds read as a tidy vite-style log.
    pub fn event(&self, path: impl Display, pages: usize, elapsed: Duration) {
        let mut s = self.state.lock();
        let clear = if self.tty { CLEAR_LINE } else { "" };
        let _ = writeln!(
            s.out,
            "{}  {}  {} {}  {} {}",
            clear,
            fmt::clock().dimmed(),
            Marker::Changed,
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

    /// A progress bar labeled `verb` over `len` items: visible only on a
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
