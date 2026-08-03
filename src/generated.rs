//! Files baudelaire writes rather than reads.
//!
//! Three producers write generated files, and they used to spell writing three
//! ways: the section and page tables a build puts in project scratch
//! ([`crate::world::generated::Table`]), the TypeScript declarations for the
//! `baudelaire:*` modules ([`crate::engine::asset::Declarations`]), and the
//! `@baudelaire/*` typst packages a mirror installs
//! ([`crate::world::module::Packages`]). Each is a set of [`File`]s under a
//! base directory, so each is one [`Generated`] impl and nothing else has to
//! know how a generated file reaches disk.
//!
//! Nothing here is ever read back by a build: a stale or deleted copy is a
//! tooling inconvenience, never a wrong page. That is what lets the same write
//! serve a build and an editor.

use std::path::{Path, PathBuf};

use crate::error::Result;

/// One generated file: where it goes, relative to the base whoever writes it
/// chooses, and what is in it.
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

    /// Write this file under `base`, creating the directories above it.
    fn write(&self, base: &Path) -> Result<()> {
        crate::fs::write_all(base.join(&self.path), &self.text)
    }
}

/// A list of producers is a producer: every package of one project, or every
/// table of one build, written together.
impl<T: Generated> Generated for Vec<T> {
    fn files(&self) -> Vec<File> {
        self.iter().flat_map(Generated::files).collect()
    }
}

/// A producer of generated files.
pub trait Generated {
    /// The files, relative to the base they are written under.
    fn files(&self) -> Vec<File>;

    /// Write every file under `base`. The one way a generated file lands on
    /// disk, so ordering, directory creation and error reporting cannot differ
    /// between a build and the `packages` command.
    fn write(&self, base: &Path) -> Result<()> {
        self.files().iter().try_for_each(|file| file.write(base))
    }
}
