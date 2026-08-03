//! The `@baudelaire/*` typst packages: machine-global, because that is where
//! typst's own resolution looks, and so the one target `--path` redirects.

use std::path::PathBuf;

use crate::error::warning::{MirrorElsewhere, MirrorUnbuilt};
use crate::error::{MirrorError, Result};
use crate::ui::{Code, List, Text};
use crate::world::module::{Package, Packages};

use super::{Advice, Mirror, Mirrored, Target};

pub(super) struct Typst;

impl Target for Typst {
    fn label(&self) -> &'static str {
        "typst modules"
    }

    fn mirrored(&self, mirror: &Mirror) -> Result<Mirrored> {
        let packages = Packages::new(mirror.config).packages();
        let modules = packages.iter().map(Package::specifier).collect();
        // The file-backed tables are built from the site's own pages, so before
        // a first build there is nothing to copy and they mirror empty. Symbols
        // resolve either way; only the values are missing, and the next run
        // after a build fills them in.
        let unbuilt: Vec<Code<&str>> = packages
            .iter()
            .filter(|package| package.empty)
            .map(|package| Code(package.name))
            .collect();
        let base = Self::directory(mirror)?;
        let mut notes: Vec<Advice> = Vec::new();
        if !unbuilt.is_empty() {
            notes.push(Box::new(MirrorUnbuilt {
                modules: List(&unbuilt).to_string(),
            }));
        }
        if mirror.dir.is_some() {
            notes.push(Box::new(MirrorElsewhere {
                dir: Text(base.display().to_string()),
            }));
        }
        Ok(Mirrored {
            base,
            generated: Box::new(packages),
            modules,
            notes,
        })
    }

    /// The package directory is shared with whatever else a reader keeps in it,
    /// `@local` packages included, so only baudelaire's own namespace directory
    /// is ever removed.
    fn owned(&self, mirror: &Mirror) -> Result<PathBuf> {
        Ok(Packages::namespace(&Self::directory(mirror)?))
    }
}

impl Typst {
    /// Where the packages go: the named directory, else typst's own.
    fn directory(mirror: &Mirror) -> Result<PathBuf> {
        match mirror.dir {
            Some(dir) => Ok(dir.to_path_buf()),
            None => Packages::directory().ok_or_else(|| MirrorError::NoDirectory.into()),
        }
    }
}
