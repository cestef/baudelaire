//! `cache { }`: the incremental build manifest. Not `headers { cache }`,
//! which is what a browser is told.

use std::path::PathBuf;

use dispatch_derive::Table;

use crate::config::Config;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

#[derive(Debug, Clone, Table)]
pub struct CacheConfig {
    /// Where incremental build state is kept.
    #[key(path)]
    pub dir: PathBuf,

    /// Reuse that state. Off, every build is a cold one.
    #[key(flag)]
    pub incremental: bool,
}

/// Keeps `incremental` out of the fingerprint: a `--no-cache` run still writes
/// the next manifest, so keying on it would make the following normal build a
/// whole-site miss.
impl std::hash::Hash for CacheConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            dir,
            incremental: _,
        } = self;
        dir.hash(state);
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            dir: Config::scratch(crate::config::Scratch::Cache),
            incremental: true,
        }
    }
}
