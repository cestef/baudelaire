//! The `baudelaire:*` TypeScript declarations: project-local, because
//! TypeScript resolves an ambient declaration through `tsconfig.json`.

use std::path::PathBuf;

use crate::engine::asset::Declarations;
use crate::error::Result;
use crate::ui::Paths;

use super::{Mirror, Mirrored, Setup, Target};

pub(super) struct Types;

impl Target for Types {
    fn label(&self) -> &'static str {
        "typescript declaration"
    }

    fn mirrored(&self, mirror: &Mirror) -> Result<Mirrored> {
        let declarations = Declarations::of(mirror.config);
        Ok(Mirrored {
            base: mirror.config.root.clone(),
            modules: declarations.modules().to_vec(),
            setup: vec![Setup {
                tool: "tsconfig",
                value: format!(
                    "add {} to the include list",
                    Paths(&Declarations::path().display().to_string())
                ),
                hint: None,
            }],
            notes: Vec::new(),
            generated: Box::new(declarations),
        })
    }

    fn owned(&self, mirror: &Mirror) -> Result<PathBuf> {
        Ok(mirror.config.root.join(Declarations::path()))
    }
}
