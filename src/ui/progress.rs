//! Progress on stderr, hidden on pipes and at any verbosity where it would
//! fight other output: a bar over work that can be counted, a spinner over a
//! phase that cannot.

use std::borrow::Cow;
use std::time::Instant;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use tracing::debug;

/// The bar's own width, in columns.
const WIDTH: usize = 24;

/// The 256-colour index the unfilled part of the bar is drawn in.
const TRACK: u8 = 238;

/// A transient progress bar over a known amount of work, erased when finished.
pub struct Progress(ProgressBar);

impl Progress {
    /// A visible bar labeled `verb` over `len` items.
    pub(super) fn bar(verb: &'static str, len: u64) -> Self {
        let bar = ProgressBar::with_draw_target(Some(len), ProgressDrawTarget::stderr());
        bar.set_style(
            ProgressStyle::with_template(&format!(
                "  {{prefix:.cyan.bold}} {{bar:{WIDTH}.magenta/{TRACK}}} {{pos}}/{{len}} {{wide_msg:.dim}}"
            ))
            .expect("the template is fixed but for two numbers, and parses")
            .progress_chars("━╸─"),
        );
        bar.set_prefix(verb);
        Self(bar)
    }

    /// A no-op bar for pipes, `--quiet`, and verbose per-page output.
    pub(super) fn hidden() -> Self {
        Self(ProgressBar::hidden())
    }

    /// One item done; `msg` names it.
    pub fn tick(&self, msg: impl Into<Cow<'static, str>>) {
        self.0.set_message(msg);
        self.0.inc(1);
    }

    pub fn finish(&self) {
        self.0.finish_and_clear();
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.finish();
    }
}

/// A phase with nothing to count: a spinner while it runs, and how long it took
/// once it is over.
///
/// The timing is the point as much as the spinner is: every phase says how long
/// it took at debug level, whether or not a terminal was there to watch it.
pub struct Step {
    what: &'static str,
    spinner: ProgressBar,
    started: Instant,
}

impl Step {
    /// The frames of the spinner, and how often they turn.
    const FRAMES: &'static [&'static str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    const TICK: u64 = 80;

    pub(super) fn spinner(what: &'static str, visible: bool) -> Self {
        let spinner = if visible {
            let spinner = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
            spinner.set_style(
                ProgressStyle::with_template("  {spinner:.cyan.bold} {msg:.dim}")
                    .expect("the template is fixed and parses")
                    .tick_strings(Self::FRAMES),
            );
            spinner.set_message(what);
            spinner.enable_steady_tick(std::time::Duration::from_millis(Self::TICK));
            spinner
        } else {
            ProgressBar::hidden()
        };
        Self {
            what,
            spinner,
            started: Instant::now(),
        }
    }
}

impl Drop for Step {
    fn drop(&mut self) {
        self.spinner.finish_and_clear();
        debug!(elapsed = ?self.started.elapsed(), "{}", self.what);
    }
}
