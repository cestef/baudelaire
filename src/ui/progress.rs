//! Compile-phase progress: a transient uv-style bar on stderr, hidden on pipes
//! and at any verbosity where it would fight other output.

use std::borrow::Cow;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

/// The bar's own width in columns: wide enough to show progress at a glance,
/// narrow enough that the label, the counter and the current item still fit on
/// one line of a narrow terminal.
const WIDTH: usize = 24;

/// The 256-colour index the unfilled part of the bar is drawn in: a dark grey
/// that reads as "not yet" without competing with the magenta fill.
const TRACK: u8 = 238;

/// A transient progress bar over a known amount of work. Safe to tick from
/// rayon workers (`indicatif`'s bar is `Sync`); it erases itself when finished
/// so the summary line is the only trace a build leaves.
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

    /// One item done; `msg` names it (shown dimmed beside the bar).
    pub fn tick(&self, msg: impl Into<Cow<'static, str>>) {
        self.0.set_message(msg);
        self.0.inc(1);
    }

    /// Erase the bar.
    pub fn finish(&self) {
        self.0.finish_and_clear();
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.finish();
    }
}
