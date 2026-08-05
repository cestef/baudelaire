//! `announce { }`: where the built site is announced.

pub mod standard;

use crate::config::StandardConfig;
use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::{Block, Section};

/// Announce destinations for the built site. Each backend is an optional
/// block under `announce { .. }`; adding a destination is one field here plus one
/// backend in [`crate::announce`]. Secrets are never stored here; a backend
/// reads its credentials from the environment at announce time.
#[derive(Debug, Clone, Hash, Default)]
pub struct AnnounceConfig {
    /// standard.site (AT Protocol) target.
    pub standard: Option<StandardConfig>,
}

/// The `announce { .. }` section: one block per destination backend.
impl Section for AnnounceConfig {
    const RULES: Block<Self> = Block(&[(
        "standard",
        Nested(StandardConfig::rows),
        "Announce to standard.site over atproto. Its presence turns it on.",
        |c, n, t| StandardConfig::optional(&mut c.standard, n, t),
    )]);
}
