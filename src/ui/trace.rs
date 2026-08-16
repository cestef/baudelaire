//! Debug logging via `tracing`: `-v` enables baudelaire's debug events, `-vv`
//! trace, and `RUST_LOG` decides for a run that passed neither.

use std::time::Instant;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

/// Millisecond-precision uptime stamp (` 0.005s`).
struct Uptime(Instant);

impl FormatTime for Uptime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{:>8.3}s", self.0.elapsed().as_secs_f64())
    }
}

/// The debug log for one run: how deep it goes, and the subscriber it installs.
#[derive(Debug, Clone, Copy)]
pub struct Logs(u8);

impl Logs {
    /// The log for a run that passed `verbosity` `-v` flags.
    pub fn new(verbosity: u8) -> Self {
        Self(verbosity)
    }

    /// The filter directive: what `-v` asked for, or `RUST_LOG` when it asked
    /// for nothing.
    fn directive(self, env: Option<&str>) -> &str {
        match (self.0, env) {
            (0, Some(env)) => env,
            (0, None) => "off",
            (1, _) => "baudelaire=debug",
            (_, _) => "baudelaire=trace",
        }
    }

    /// Install the global subscriber; called once, at CLI startup.
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
    /// stderr, which is where that question is decided for the whole run.
    fn color() -> bool {
        anstream::AutoStream::choice(&std::io::stderr()) != anstream::ColorChoice::Never
    }
}

#[cfg(test)]
mod tests {
    use super::Logs;

    #[test]
    fn a_verbosity_flag_beats_the_environment() {
        assert_eq!(Logs::new(2).directive(Some("warn")), "baudelaire=trace");
        assert_eq!(Logs::new(1).directive(Some("warn")), "baudelaire=debug");
        assert_eq!(Logs::new(0).directive(Some("warn")), "warn");
        assert_eq!(Logs::new(0).directive(None), "off");
    }
}
