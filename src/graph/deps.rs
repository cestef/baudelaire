//! A page's compile-time file dependencies.

use std::path::PathBuf;

/// The files a page's compilation read (transitive imports, data loaders, and
/// assets) as captured by [`crate::world::Tracked`].
#[derive(Debug, Default)]
pub struct Deps {
    files: Vec<PathBuf>,
}

impl Deps {
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Add files the compilation itself never read, for the passes that load
    /// something typst does not see (an inlined SVG icon).
    pub fn extend(&mut self, files: impl IntoIterator<Item = PathBuf>) {
        self.files.extend(files);
    }
}

impl From<Vec<PathBuf>> for Deps {
    fn from(files: Vec<PathBuf>) -> Self {
        Self { files }
    }
}
