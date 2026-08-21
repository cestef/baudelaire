//! `content { history { } }`: what git knows about each page.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// A page's own history, handed to templates as `page.git`. Enabled by the
/// presence of a `content { history }` block.
///
/// Off by default because reading it walks the repository's whole log, once per
/// build.
#[derive(Debug, Clone, Hash, Default, Table)]
#[table(hook(switch = enabled))]
pub struct HistoryConfig {
    pub enabled: bool,

    /// Gather everyone who has changed a page, not only the commit that last did.
    #[key(flag)]
    pub contributors: bool,
}
