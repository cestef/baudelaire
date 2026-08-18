//! Terminal output: one shared, thread-safe [`Ui`] behind every line the CLI
//! prints, on stderr, since stdout stays reserved for `--json` data.

mod fmt;
mod highlight;
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
pub use highlight::Highlighted;
pub use marker::{Marker, PageStatus};
pub(crate) use markup::markup;
pub use markup::{Code, Markup, Styled, Text};
pub use progress::Progress;

/// Return the cursor to column 0 and erase the line; only ever written to a
/// tty, where on a pipe it would strand the escape in the log.
const CLEAR_LINE: &str = "\r\x1b[2K";

/// The width `➜` labels are padded to, sized to the longest label in use.
const ARROW_LABEL: usize = 11;

/// The column an arrow's value starts at, where a caller aligns the
/// continuations of a multi-line value.
pub const ARROW_VALUE_COLUMN: usize = 2 + 1 + 1 + ARROW_LABEL + 1;

/// The band the diagnostic renderer's width is clamped into, and what it uses
/// when the terminal size is unavailable.
const REPORT_MIN_WIDTH: usize = 60;
const REPORT_MAX_WIDTH: usize = 120;
const REPORT_NO_TERMINAL_WIDTH: usize = 96;

/// Columns [`Ui::flush`] indents a rendered line by, counted twice so the box
/// keeps the same margin on its right.
const REPORT_MARGIN: usize = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Nothing but collected warnings.
    Silent,
    /// Only warnings and the final result.
    Quiet,
    /// Banner, results, warnings.
    #[default]
    Default,
    /// Adds per-page progress and detail.
    Verbose,
}

pub struct Timer(Instant);

impl Timer {
    pub fn start() -> Self {
        Self(Instant::now())
    }

    pub fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

/// What a run produced, as `--json` writes it to stdout.
#[derive(serde::Serialize)]
pub struct Report {
    pub schema: u32,
    /// Whether the run succeeded. A `--strict` failure is still `false`.
    pub ok: bool,
    /// Absent for a command that builds nothing (`clean`, `new`, `init`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<usize>,
    pub warnings: usize,
    /// Every diagnostic collected, in the order they were reported.
    pub diagnostics: Vec<Diagnostics>,
}

impl Report {
    /// The version of the `--json` contract, bumped when an existing field
    /// changes meaning, changes type, or goes away, never when one is added.
    pub const SCHEMA: u32 = 1;

    /// Write this report to stdout; a serialization failure is swallowed
    /// rather than replacing the run's real outcome.
    pub fn emit(&self) {
        if let Ok(text) = serde_json::to_string(self) {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{text}");
            let _ = out.flush();
        }
    }
}

/// One collected diagnostic, reduced to what a machine can act on.
#[derive(serde::Serialize, Clone)]
pub struct Diagnostics {
    /// Absent for a diagnostic carrying no code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub severity: &'static str,
    pub message: String,
}

impl Diagnostics {
    fn of(diagnostic: &dyn Diagnostic) -> Self {
        Self {
            code: diagnostic.code().map(|c| c.to_string()),
            severity: match diagnostic.severity().unwrap_or(Severity::Error) {
                Severity::Advice => "advice",
                Severity::Warning => "warning",
                Severity::Error => "error",
            },
            message: Markup::new(&diagnostic.to_string(), false).to_string(),
        }
    }
}

/// A collected diagnostic; warnings tally into the build summary, advice does
/// not.
struct Note(Box<dyn Diagnostic + Send + Sync>);

impl Note {
    fn is_warning(&self) -> bool {
        self.0.severity().unwrap_or(Severity::Error) >= Severity::Warning
    }
}

struct State {
    out: AutoStream<std::io::Stderr>,
    level: Level,
    notes: Vec<Note>,
    warned: usize,
    /// What the run produced, absent for a command that counts nothing.
    built: Option<(usize, usize)>,
    /// Kept apart from `notes` because [`Ui::flush`] drains those long before
    /// the `--json` report is assembled.
    collected: Vec<Diagnostics>,
}

/// The shared terminal reporter, threaded by `&self` through every worker.
pub struct Ui {
    state: Mutex<State>,
    tty: bool,
    /// Whether the writer will keep colour, needed up front because [`Markup`]
    /// renders a span structurally rather than stripping escapes afterwards.
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

    pub fn level(&self) -> Level {
        self.state.lock().level
    }

    pub fn set_level(&self, level: Level) {
        self.state.lock().level = level;
    }

    pub fn warnings(&self) -> usize {
        self.state.lock().warned
    }

    /// Record what a build produced, for `--json`.
    pub fn built(&self, pages: usize, cached: usize) {
        self.state.lock().built = Some((pages, cached));
    }

    /// Record the failure that ended this run, for the `--json` report alone:
    /// printing it is `main`'s job.
    pub fn failed(&self, error: &dyn Diagnostic) {
        self.state.lock().collected.push(Diagnostics::of(error));
    }

    /// The machine-readable record of this run, which the caller must build
    /// before [`flush`](Ui::flush) drains the notes.
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

    /// The command banner: `baudelaire v0.1.0  building my-site`.
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

    /// A result line: `✓ built 24 pages .. in 132ms`.
    pub fn done(&self, msg: impl Display) {
        self.done_inner(msg, true);
    }

    /// Like [`done`](Self::done) but flush left.
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

    /// Rows hung off the preceding result as a tree, the last one rounded.
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

    /// A vite-style pointer line: `➜ local  http://..`, its label padded so
    /// consecutive arrows align.
    pub fn arrow(&self, label: &str, value: impl Display) {
        self.arrow_inner(label, value, Level::Default);
    }

    /// The same line, shown at every level but [`Level::Silent`].
    pub fn arrow_kept(&self, label: &str, value: impl Display) {
        self.arrow_inner(label, value, Level::Quiet);
    }

    fn arrow_inner(&self, label: &str, value: impl Display, least: Level) {
        let mut s = self.state.lock();
        if s.level < least {
            return;
        }
        let _ = writeln!(
            s.out,
            "  {} {} {}",
            Marker::Pointer,
            format!("{label:<ARROW_LABEL$}").bold(),
            value
        );
    }

    /// A blank line, for vertical grouping.
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

    /// Collect a warning, rendered by the next [`Ui::flush`] and counted in
    /// the build summary.
    pub fn warn(&self, warning: impl Diagnostic + Send + Sync + 'static) {
        self.report(Box::new(warning));
    }

    /// Collect an already-boxed warning.
    pub fn report(&self, warning: Box<dyn Diagnostic + Send + Sync>) {
        let mut s = self.state.lock();
        let note = Note(warning);
        s.warned += usize::from(note.is_warning());
        s.collected.push(Diagnostics::of(&*note.0));
        s.notes.push(note);
    }

    /// Collect an informational note, rendered with the warnings but never
    /// counted against the build.
    pub fn advice(&self, advice: impl Diagnostic + Send + Sync + 'static) {
        self.warn(advice);
    }

    /// Render everything collected since the last flush, identical renders
    /// collapsed into one block with a repeat count.
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

    /// Render the error that ended the run; the one place a failure prints.
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

    /// A diagnostic rendered at a fixed width and without ANSI escapes, for a
    /// reader that is not this terminal.
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

    /// One diagnostic, formatted, falling back to the bare message if the
    /// handler itself fails.
    fn render(&self, diagnostic: &dyn Diagnostic) -> String {
        let styled = Styled::new(diagnostic, self.color);
        let mut text = String::new();
        match Self::handler().render_report(&mut text, &styled) {
            Ok(()) => text,
            Err(_) => styled.to_string(),
        }
    }

    /// The renderer for collected diagnostics, sized to the terminal.
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
    /// output and skipped on a pipe, which cannot take it back.
    pub fn status(&self, msg: impl Display) {
        let mut s = self.state.lock();
        if s.level < Level::Default || !self.tty {
            return;
        }
        let _ = write!(s.out, "{CLEAR_LINE}  {} {}", Marker::Working, msg.dimmed());
        let _ = s.out.flush();
    }

    /// A dev-server event line: wall clock, change glyph, the file that
    /// triggered the rebuild, and what it cost.
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

    /// A dev-server request that missed (verbose+): `12:31:02 404 /x.ico`.
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

    /// A progress bar labeled `verb` over `len` items, visible only on a
    /// terminal at the default level.
    pub fn progress(&self, verb: &'static str, len: usize) -> Progress {
        if self.tty && self.level() == Level::Default && len > 0 {
            Progress::bar(verb, len as u64)
        } else {
            Progress::hidden()
        }
    }
}
