//! `baudelaire build`: compile the site into the output directory.

use clap::Args;

use super::{BuildOverrides, Cx, Run};
use crate::error::Result;

/// Arguments for `baudelaire build`.
#[derive(Args, Debug, Clone, Default)]
pub struct BuildArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,
}

impl Run for BuildArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "building")?;
        let stats = crate::engine::Engine::new(config, crate::engine::Mode::Build)?.build(cx.ui)?;
        cx.ui.built(stats.pages, stats.cached);
        Ok(())
    }
}
