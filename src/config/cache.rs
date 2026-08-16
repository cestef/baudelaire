//! `cache { }`: the incremental build manifest. Not [`super::caching`].

use std::path::PathBuf;

use crate::config::Config;
use crate::config::dispatch::Kind::{Flag, Path};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub dir: PathBuf,
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

impl Section for CacheConfig {
    const RULES: Block<Self> = Block(&[
        (
            "dir",
            Path,
            "Where incremental build state is kept.",
            |c, n, t| {
                c.dir = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "incremental",
            Flag,
            "Reuse that state. Off, every build is a cold one.",
            |c, n, t| {
                c.incremental = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
