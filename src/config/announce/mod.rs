//! `announce { }`: where the built site is announced.

pub mod standard;

use crate::config::StandardConfig;
use crate::config::Value;
use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::{Block, Section};

/// Announce destinations for the built site, one optional block per backend.
/// Secrets are never stored here; a backend reads its credentials from the
/// environment at announce time.
#[derive(Debug, Clone, Hash, Default)]
pub struct AnnounceConfig {
    /// standard.site (AT Protocol) target.
    pub standard: Option<StandardConfig>,
}

impl Section for AnnounceConfig {
    const RULES: Block<Self> = Block(&[(
        "standard",
        Nested(StandardConfig::rows),
        "Announce to standard.site over atproto. Its presence turns it on.",
        |c| c.standard.as_ref().map_or(Value::Unset, Section::values),
        |c, n, t| StandardConfig::optional(&mut c.standard, n, t),
    )]);
}
