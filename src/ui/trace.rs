//! Debug logging via `tracing`: `-v` enables baudelaire's debug events, `-vv`
//! trace. `RUST_LOG` decides for a run that passed neither, and is the only way
//! to see events from dependencies.
//!
//! Strictly diagnostic: user-facing output goes through [`super::Ui`]. Events
//! land on stderr with an uptime stamp so build phases can be profiled at a
//! glance.

use std::io::IsTerminal;
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

/// The filter directive for a run: what `-v` asked for, or `RUST_LOG` when it
/// asked for nothing.
///
/// A flag beats the environment, the same precedence the rest of the CLI
/// follows. It used to be the inverse: any `RUST_LOG` at all discarded the `-v`
/// count, so `RUST_LOG=warn baudelaire -vv build` printed no debug events and
/// never said why.
fn directive(verbosity: u8, env: Option<&str>) -> &str {
    match (verbosity, env) {
        (0, Some(env)) => env,
        (0, None) => "off",
        (1, _) => "baudelaire=debug",
        (_, _) => "baudelaire=trace",
    }
}

/// Install the global subscriber. Called once at CLI startup; `verbosity` is
/// the count of `-v` flags.
pub fn init(verbosity: u8) {
    let env = std::env::var("RUST_LOG").ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(directive(verbosity, env.as_deref())))
        .with_writer(std::io::stderr)
        .with_timer(Uptime(Instant::now()))
        .with_target(verbosity > 1)
        .with_ansi(std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none())
        .compact()
        .init();
}

#[cfg(test)]
mod tests {
    use super::directive;

    /// An explicit `-v` wins over the ambient `RUST_LOG`, and a run that passed
    /// no flag still honours it: the flag-beats-environment order every other
    /// setting in the CLI uses.
    #[test]
    fn a_verbosity_flag_beats_the_environment() {
        assert_eq!(directive(2, Some("warn")), "baudelaire=trace");
        assert_eq!(directive(1, Some("warn")), "baudelaire=debug");
        assert_eq!(directive(0, Some("warn")), "warn");
        assert_eq!(directive(0, None), "off");
    }
}
