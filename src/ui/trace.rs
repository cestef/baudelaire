//! Debug logging via `tracing`: `-v` enables baudelaire's debug events, `-vv`
//! trace; `RUST_LOG` overrides both (and is the only way to see dependencies).
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

/// Install the global subscriber. Called once at CLI startup; `verbosity` is
/// the count of `-v` flags.
pub fn init(verbosity: u8) {
    let default = match verbosity {
        0 => "off",
        1 => "baudelaire=debug",
        _ => "baudelaire=trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_timer(Uptime(Instant::now()))
        .with_target(verbosity > 1)
        .with_ansi(std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none())
        .compact()
        .init();
}
