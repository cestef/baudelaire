//! Files baudelaire writes rather than reads: nothing here is read back by a
//! build, so a stale or deleted copy can never be a wrong page.

use std::path::{Path, PathBuf};

use crate::error::Result;

/// One generated file, its path relative to the base it is written under.
pub struct File {
    pub path: PathBuf,
    pub text: String,
}

impl File {
    pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
        }
    }

    /// Writes under `base`, creating the directories above it.
    fn write(&self, base: &Path) -> Result<()> {
        crate::fs::write_all(base.join(&self.path), &self.text)
    }
}

impl<T: Generated> Generated for Vec<T> {
    fn files(&self) -> Vec<File> {
        self.iter().flat_map(Generated::files).collect()
    }
}

pub trait Generated {
    /// The files, relative to the base they are written under.
    fn files(&self) -> Vec<File>;

    fn write(&self, base: &Path) -> Result<()> {
        self.files().iter().try_for_each(|file| file.write(base))
    }
}
