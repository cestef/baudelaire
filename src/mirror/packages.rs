//! The `@baudelaire/*` typst packages: machine-global, because that is where
//! typst's own resolution looks, and so the one target `--path` redirects.

use std::path::PathBuf;

use crate::error::warning::MirrorUnbuilt;
use crate::error::{MirrorError, Result};
use crate::ui::{Code, List, Paths};
use crate::world::module::{Package, Packages};

use super::{Advice, Mirror, Mirrored, Setup, Target};

pub(super) struct Typst;

impl Target for Typst {
    fn label(&self) -> &'static str {
        "typst module"
    }

    fn mirrored(&self, mirror: &Mirror) -> Result<Mirrored> {
        let packages = Packages::new(mirror.config).packages();
        let modules = packages.iter().map(Package::specifier).collect();
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
        let setup = if mirror.global {
            Vec::new()
        } else {
            vec![Setup {
                tool: "typst",
                value: format!(
                    "--package-path {}",
                    Paths(&crate::fs::resolved(&base).display().to_string())
                ),
                hint: Some("or TYPST_PACKAGE_PATH; tinymist takes it in typstExtraArgs"),
            }]
        };
        Ok(Mirrored {
            base,
            generated: Box::new(packages),
            modules,
            setup,
            notes,
        })
    }

    /// Only baudelaire's own namespace directory, since the package directory
    /// is shared with a reader's `@local` packages.
    fn owned(&self, mirror: &Mirror) -> Result<PathBuf> {
        Ok(Packages::namespace(&Self::directory(mirror)?))
    }
}

impl Typst {
    /// Where the packages go: the named directory, else typst's own under
    /// `--global`, else this project's own.
    fn directory(mirror: &Mirror) -> Result<PathBuf> {
        match (mirror.dir, mirror.global) {
            (Some(dir), _) => Ok(dir.to_path_buf()),
            (None, true) => Packages::directory().ok_or_else(|| MirrorError::NoDirectory.into()),
            (None, false) => Ok(mirror.config.root.join(Packages::project())),
        }
    }
}
