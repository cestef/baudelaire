//! Debug logging via `tracing`: `-v` enables baudelaire's debug events, `-vv`
//! trace. `RUST_LOG` decides for a run that passed neither, and is the only way
//! to see events from dependencies.
//!
//! Strictly diagnostic: user-facing output goes through [`super::Ui`]. Events
//! land on stderr with an uptime stamp so build phases can be profiled at a
//! glance.

use std::time::Instant;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

/// Millisecond-precision uptime stamp (` 0.005s`): enough to profile build
/// phases without the nanosecond noise of the stock uptime timer.
struct Uptime(Instant);

impl FormatTime for Uptime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{:>8.3}s", self.0.elapsed().as_secs_f64())
    }
}

/// The debug log for one run: how deep it goes, and installing the subscriber
/// that writes it.
///
/// A run has exactly one, so this is a value rather than a pair of functions
/// over a loose `u8`: the depth is asked for once, at startup, and everything
/// the subscriber needs is derived from it.
#[derive(Debug, Clone, Copy)]
pub struct Logs(u8);

impl Logs {
    /// The log for a run that passed `verbosity` `-v` flags.
    pub fn new(verbosity: u8) -> Self {
        Self(verbosity)
    }

    /// The filter directive: what `-v` asked for, or `RUST_LOG` when it asked
    /// for nothing.
    ///
    /// A flag beats the environment, the same precedence the rest of the CLI
    /// follows. It used to be the inverse: any `RUST_LOG` at all discarded the
    /// `-v` count, so `RUST_LOG=warn baudelaire -vv build` printed no debug
    /// events and never said why.
    fn directive(self, env: Option<&str>) -> &str {
        match (self.0, env) {
            (0, Some(env)) => env,
            (0, None) => "off",
            (1, _) => "baudelaire=debug",
            (_, _) => "baudelaire=trace",
        }
    }

    /// Install the global subscriber. Called once at CLI startup.
    ///
    /// Colour is asked of the stream the same way every other line asks: a
    /// terminal, minus whatever the environment and `--color` have already
    /// settled. It used to test `NO_COLOR` itself, which was a second, narrower
    /// copy of that policy: `--color=never` and `CLICOLOR=0` left these events
    /// styled while the rest of the run went plain.
    pub fn install(self) {
        let env = std::env::var("RUST_LOG").ok();
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(self.directive(env.as_deref())))
            .with_writer(std::io::stderr)
            .with_timer(Uptime(Instant::now()))
            .with_target(self.0 > 1)
            .with_ansi(Self::color())
            .compact()
            .init();
    }

    /// Whether these events keep their colour: the answer `anstream` gives for
    /// stderr, which is the one place that question is decided. It already
    /// folds in the terminal check, `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`
    /// and whatever `--color` installed over them.
    fn color() -> bool {
        anstream::AutoStream::choice(&std::io::stderr()) != anstream::ColorChoice::Never
    }
}

#[cfg(test)]
mod tests {
    use super::Logs;

    /// An explicit `-v` wins over the ambient `RUST_LOG`, and a run that passed
    /// no flag still honours it: the flag-beats-environment order every other
    /// setting in the CLI uses.
    #[test]
    fn a_verbosity_flag_beats_the_environment() {
        assert_eq!(Logs::new(2).directive(Some("warn")), "baudelaire=trace");
        assert_eq!(Logs::new(1).directive(Some("warn")), "baudelaire=debug");
        assert_eq!(Logs::new(0).directive(Some("warn")), "warn");
        assert_eq!(Logs::new(0).directive(None), "off");
    }
}
