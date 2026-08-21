//! `hooks { }`: external commands run around the build.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// External command hooks, each run through the system shell in the project
/// root.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct HooksConfig {
    /// Commands run before the asset pipeline, so what they generate is picked up.
    #[key(texts)]
    pub before: Vec<String>,

    /// Commands run once the output directory is written.
    #[key(texts)]
    pub after: Vec<String>,
}
