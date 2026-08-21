//! `announce { }`: where the built site is announced.

pub mod standard;

use dispatch_derive::Table;

use crate::config::StandardConfig;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// Announce destinations for the built site, one optional block per backend.
/// Secrets are never stored here; a backend reads its credentials from the
/// environment at announce time.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct AnnounceConfig {
    /// Announce to standard.site over atproto. Its presence turns it on.
    #[key(opt nested(StandardConfig))]
    pub standard: Option<StandardConfig>,
}
