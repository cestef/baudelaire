//! Terminal output: one shared, thread-safe [`Ui`] behind every line the CLI
//! prints.
//!
//! Layers:
//! - **Reporting** ([`Ui`]): banners, results, per-page progress, dev-server
//!   event lines. Human-facing, on stderr, level-gated.
//! - **Warnings** ([`Ui::warn`]): full [`miette::Diagnostic`]s with codes,
//!   spans, and help, collected during a run and rendered together by
//!   [`Ui::flush`], so a build's noise never interleaves with its progress.
//! - **Errors** ([`Ui::fail`]): the diagnostic that ended the run, through the
//!   same renderer. `main` returns an exit code rather than a
//!   [`miette::Result`] so there is one of these and not two.
//! - **Debug logs** ([`trace`]): `tracing` events for `-v`/`-vv`/`RUST_LOG`,
//!   strictly diagnostic.
//!
//! What a line is made of lives beside it: `marker` is the one table of status
//! glyphs and their colours, `fmt` the count, size, duration and path adapters
//! every line formats through, `markup` the `` `code` `` / `*bold*` grammar
//! diagnostics are written in (with the [`Text`] and [`Code`] adapters that keep
//! an interpolated value from being read as markup), and `progress` the compile
//! bar.
//!
//! Everything goes to stderr (stdout stays reserved for data), through
//! `anstream` so color strips on pipes and under `NO_COLOR`. Output is
//! best-effort by design: a failed terminal write never fails a build, so every
//! method here is infallible.

mod fmt;
mod marker;
mod markup;
mod progress;
pub mod trace;

use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use anstream::{AutoStream, ColorChoice};
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, Severity};
use owo_colors::OwoColorize;
use parking_lot::Mutex;

pub use fmt::{Bytes, Count, Dur, List, Paths, Wrap};
pub use marker::{Marker, PageStatus};
pub(crate) use markup::markup;
pub use markup::{Code, Markup, Styled, Text};
pub use progress::Progress;

/// Return the cursor to column 0 and erase the line: what makes a transient
/// status line transient. Written raw because it is cursor control rather than
/// styling, so `anstream` has nothing to strip; every use is guarded by a tty
/// check, since on a pipe it would strand the escape in the log.
const CLEAR_LINE: &str = "\r\x1b[2K";

/// The width `➜` labels are padded to, so consecutive arrows line their values
/// up. Sized to the longest label in use, which is `collections` in `theme
/// info`; it read 9 (`watching`) while that block printed two labels wider than
/// itself, so those two rows alone were pushed out of the column.
const ARROW_LABEL: usize = 11;

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

/// What a run produced, as data rather than as prose: what `--json` writes to
/// stdout.
///
/// stdout was already reserved for exactly this and had never carried anything,
/// so a CI job could get a page count, a broken-link list or a warning set only
/// by scraping styled text off stderr.
#[derive(serde::Serialize)]
pub struct Report {
    /// The shape of this object, so a consumer can refuse one it does not
    /// understand instead of silently reading a field that moved. First, so it
    /// is the first thing in the output as well as the first thing to check.
    pub schema: u32,
    /// Whether the run succeeded. A `--strict` failure is still `false`.
    pub ok: bool,
    /// Absent for a command that builds nothing (`clean`, `new`, `init`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<usize>,
    pub warnings: usize,
    /// Every diagnostic collected, warnings and advice alike, in the order they
    /// were reported.
    pub diagnostics: Vec<Diagnostics>,
}

impl Report {
    /// The version of the `--json` contract, carried in every report as
    /// [`schema`](Report::schema).
    ///
    /// Bumped when an existing field changes meaning, changes type, or goes
    /// away. Adding a field does *not* bump it: a consumer reading by name is
    /// unaffected by a new sibling, and treating additions as breaking would
    /// train everyone to ignore the number.
    ///
    /// The reason it exists before anyone parses this output is that it cannot
    /// be added afterwards. A consumer written against an unversioned object
    /// has no way to tell a v1 report from a v2 one, so the first breaking
    /// change breaks it silently; a consumer that has always seen `schema` can
    /// refuse what it does not know.
    pub const SCHEMA: u32 = 1;

    /// Write this report to stdout, the one thing that ever goes there.
    ///
    /// A serialization failure is swallowed rather than replacing the run's
    /// real outcome: the report describes what happened, and losing the
    /// description is not the same as the thing having failed.
    pub fn emit(&self) {
        if let Ok(text) = serde_json::to_string(self) {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{text}");
            let _ = out.flush();
        }
    }
}

/// One collected diagnostic, reduced to what a machine can act on: which class
/// it is, how much it matters, and what it said.
#[derive(serde::Serialize, Clone)]
pub struct Diagnostics {
    /// The `baudelaire::..` code, absent only for a diagnostic carrying none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub severity: &'static str,
    pub message: String,
}

impl Diagnostics {
    fn of(note: &Note) -> Self {
        Self {
            code: note.0.code().map(|c| c.to_string()),
            severity: match note.0.severity().unwrap_or(Severity::Error) {
                Severity::Advice => "advice",
                Severity::Warning => "warning",
                Severity::Error => "error",
            },
            // The plain rendering, not the styled one: a JSON consumer wants the
            // text, and markup delimiters are this terminal's business.
            message: Markup::new(&note.0.to_string(), false).to_string(),
        }
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
    /// What the run produced, when it produced anything countable. Recorded by
    /// the commands that build, read only by [`Ui::summary`].
    built: Option<(usize, usize)>,
    /// Every diagnostic collected this run, in machine-readable form.
    ///
    /// Kept apart from `notes` because `flush` drains those, and a build flushes
    /// its own warnings well before the CLI assembles the `--json` report: read
    /// from `notes`, the report came out empty while the count said otherwise.
    collected: Vec<Diagnostics>,
}

/// The shared terminal reporter. All methods take `&self` (state sits behind a
/// mutex), so one `Ui` threads freely through rayon workers and the dev
/// server's request thread.
pub struct Ui {
    state: Mutex<State>,
    tty: bool,
    /// Whether the writer will keep colour, asked of `anstream` with the same
    /// question it answers for itself. Diagnostics need it up front rather than
    /// after the fact: [`Markup`] renders a span *structurally*, styling it here
    /// and writing its delimiters back out there, so stripping the escapes off a
    /// coloured render would leave a path with no quotes around it at all.
    color: bool,
}

impl Ui {
    pub fn new(level: Level) -> Self {
        Self {
            state: Mutex::new(State {
                out: AutoStream::auto(std::io::stderr()),
                level,
                notes: Vec::new(),
                warned: 0,
                built: None,
                collected: Vec::new(),
            }),
            tty: std::io::stderr().is_terminal(),
            color: AutoStream::choice(&std::io::stderr()) != ColorChoice::Never,
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

    /// Record what a build produced, for `--json`. Called by the commands that
    /// build; everything else reports no counts rather than zero ones, which
    /// are different claims.
    pub fn built(&self, pages: usize, cached: usize) {
        self.state.lock().built = Some((pages, cached));
    }

    /// The machine-readable record of this run.
    ///
    /// Built *before* [`flush`](Ui::flush) drains the notes, and written to
    /// stdout, which is why nothing else here ever writes there: a caller
    /// piping `--json` gets one object and no prose mixed into it.
    pub fn summary(&self, ok: bool) -> Report {
        let s = self.state.lock();
        Report {
            schema: Report::SCHEMA,
            ok,
            pages: s.built.map(|(pages, _)| pages),
            cached: s.built.map(|(_, cached)| cached),
            warnings: s.warned,
            diagnostics: s.collected.clone(),
        }
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
        s.collected.push(Diagnostics::of(&note));
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
        let mut seen: Vec<(String, usize)> = Vec::new();
        for note in &notes {
            let text = self.render(&*note.0);
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

    /// Render the error that ended the run, into the same report column its
    /// warnings use. Printed at every level: an error is never quieted, and
    /// [`Ui::flush`] has already emptied the warnings above it.
    ///
    /// The one place a failure prints. `main` returns an exit code rather than a
    /// [`miette::Result`] so that this renderer, not miette's global hook,
    /// decides the width, the colour and the markup.
    pub fn fail(&self, error: &dyn Diagnostic) {
        let text = self.render(error);
        let mut s = self.state.lock();
        if self.tty {
            let _ = write!(s.out, "{CLEAR_LINE}");
        }
        let _ = writeln!(s.out);
        for line in text.lines() {
            let _ = writeln!(s.out, "  {line}");
        }
        let _ = writeln!(s.out);
    }

    /// A diagnostic rendered as plain text, for a reader that is not this
    /// terminal.
    ///
    /// The dev server's browser overlay is the caller: a page cannot interpret
    /// ANSI escapes, and the point of putting the diagnostic on screen is that
    /// it reads exactly like the one in the terminal, spans and all. Fixed
    /// width for the same reason: the browser's viewport is not this terminal's.
    pub fn plain(diagnostic: &dyn Diagnostic) -> String {
        let styled = Styled::new(diagnostic, false);
        let mut text = String::new();
        let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
            .with_width(REPORT_NO_TERMINAL_WIDTH);
        match handler.render_report(&mut text, &styled) {
            Ok(()) => text,
            Err(_) => styled.to_string(),
        }
    }

    /// One diagnostic, formatted: its markup rendered by [`Styled`], the rest by
    /// miette. Falls back to the bare message if the handler itself fails.
    fn render(&self, diagnostic: &dyn Diagnostic) -> String {
        let styled = Styled::new(diagnostic, self.color);
        let mut text = String::new();
        match Self::handler().render_report(&mut text, &styled) {
            Ok(()) => text,
            Err(_) => styled.to_string(),
        }
    }

    /// The renderer for collected diagnostics, sized to the terminal. Colors
    /// are always emitted: the `anstream` writer strips them on pipes and
    /// under `NO_COLOR`, same as every other line.
    fn handler() -> GraphicalReportHandler {
        let width =
            console::Term::stderr()
                .size_checked()
                .map_or(REPORT_NO_TERMINAL_WIDTH, |(_, cols)| {
                    usize::from(cols)
                        .saturating_sub(REPORT_MARGIN)
                        .clamp(REPORT_MIN_WIDTH, REPORT_MAX_WIDTH)
                });
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
