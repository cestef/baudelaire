//! Shared date formatting.

use std::fmt;

/// A date as an ISO-8601 day (`2026-07-15`) — the single date rendering used by
/// listings, the JS `baudelaire:pages`/`baudelaire:feed` modules, and (with a
/// midnight-UTC suffix) publish timestamps.
pub struct Iso(pub time::Date);

impl fmt::Display for Iso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}",
            self.0.year(),
            u8::from(self.0.month()),
            self.0.day()
        )
    }
}
