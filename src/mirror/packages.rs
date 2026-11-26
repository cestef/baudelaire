//! The generated typst modules, written under the project where a template's
//! `/.baudelaire/site/<name>.typ` import already points.

use std::path::PathBuf;

use crate::error::Result;
use crate::error::warning::MirrorUnbuilt;
use crate::ui::{Code, List};
use crate::world::module::{Mirrored as Modules, Package};

use super::{Advice, Mirror, Mirrored, Target};

pub(super) struct Typst;

impl Target for Typst {
    fn label(&self) -> &'static str {
        "typst module"
    }

    fn mirrored(&self, mirror: &Mirror) -> Result<Mirrored> {
        let packages = Modules::new(mirror.config).packages();
        let modules = packages.iter().map(Package::specifier).collect();
        let unbuilt: Vec<Code<&str>> = packages
            .iter()
            .filter(|package| package.empty)
            .map(|package| Code(package.name))
            .collect();
        let mut notes: Vec<Advice> = Vec::new();
        if !unbuilt.is_empty() {
            notes.push(Box::new(MirrorUnbuilt {
                modules: List(&unbuilt).to_string(),
            }));
        }
        Ok(Mirrored {
            base: mirror.config.root.clone(),
            generated: Box::new(packages),
            modules,
            setup: Vec::new(),
            notes,
        })
    }

    fn owned(&self, mirror: &Mirror) -> Result<PathBuf> {
        Ok(mirror.config.root.join(Modules::directory()))
    }
}
