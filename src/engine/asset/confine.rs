//! The directories an asset compiler may read an import from: the project and
//! every asset layer, the theme's included even where it lives outside.

use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use crate::config::Config;
use crate::fs;

/// The canonical roots an import must resolve under, and the last path it
/// refused, which a compiler that swallows a hook's message is asked for.
#[derive(Debug, Default)]
pub(super) struct Confine {
    roots: Vec<PathBuf>,
    refused: Mutex<Option<PathBuf>>,
}

impl Confine {
    pub(super) fn new(config: &Config, layers: &[PathBuf]) -> Self {
        Self {
            roots: std::iter::once(&config.root)
                .chain(layers)
                .filter_map(|root| fs::canonicalize(root).ok())
                .collect(),
            refused: Mutex::default(),
        }
    }

    /// Whether `path` names an existing file or directory outside every root,
    /// recording it if so. A path naming nothing is not outside: the compiler
    /// reports it missing.
    pub(super) fn outside(&self, path: &Path) -> bool {
        let outside = fs::canonicalize(path)
            .is_ok_and(|real| !self.roots.iter().any(|root| real.starts_with(root)));
        if outside {
            *self.refused.lock() = Some(path.to_path_buf());
        }
        outside
    }

    /// The path refused since the last call, if any.
    pub(super) fn refused(&self) -> Option<PathBuf> {
        self.refused.lock().take()
    }
}
