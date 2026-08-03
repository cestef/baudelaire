//! The `baudelaire:*` TypeScript declarations: project-local, because
//! TypeScript resolves an ambient declaration through `tsconfig.json` and a
//! machine-global copy would be one nothing includes. `--path` does not reach
//! this target, and `clean`, which sweeps project state, takes it with it.
//!
//! Every build writes this file too (see [`crate::engine`]), so the types
//! follow the config without anyone re-running a command; mirroring it here is
//! what covers a checkout that has not been built yet.

use std::path::PathBuf;

use crate::engine::asset::Declarations;
use crate::error::Result;
use crate::error::warning::MirrorInclude;

use super::{Mirror, Mirrored, Target};

pub(super) struct Types;

impl Target for Types {
    fn label(&self) -> &'static str {
        "typescript declarations"
    }

    fn mirrored(&self, mirror: &Mirror) -> Result<Mirrored> {
        let declarations = Declarations::of(mirror.config);
        Ok(Mirrored {
            base: mirror.config.root.clone(),
            modules: declarations.modules().to_vec(),
            notes: vec![Box::new(MirrorInclude {
                path: Declarations::path().display().to_string(),
            })],
            generated: Box::new(declarations),
        })
    }

    fn owned(&self, mirror: &Mirror) -> Result<PathBuf> {
        Ok(mirror.config.root.join(Declarations::path()))
    }
}
