//! A page's compile-time file dependencies.

use std::path::PathBuf;

/// The files a page's compilation read — transitive imports, data loaders, and
/// assets — as captured by [`crate::world::Tracked`]. A change to any of them
/// invalidates the page's cached output.
#[derive(Debug, Default)]
pub struct Deps {
    files: Vec<PathBuf>,
}

impl Deps {
    /// Build from the resolved paths of a compilation's accessed files.
    pub fn from_paths(files: Vec<PathBuf>) -> Self {
        Self { files }
    }

    /// The dependency files.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}
