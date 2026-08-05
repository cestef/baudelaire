//! `baudelaire check`: build into a scratch directory and report, writing nothing.

use clap::Args;

use super::{CommonOverrides, Cx, Run, Toggle, group};
use crate::error::Result;

/// Arguments for `baudelaire check`.
#[derive(Args, Debug, Clone)]
pub struct CheckArgs {
    #[command(flatten)]
    pub overrides: CommonOverrides,

    /// Also verify outbound `http(s)` links over the network
    /// (`--no-external` skips them even when the config asks).
    #[arg(long, overrides_with = "no_external", help_heading = group::BUILD)]
    pub external: bool,
    #[arg(long, overrides_with = "external", hide = true)]
    pub no_external: bool,
}

impl Run for CheckArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let mut config = cx.configured(&self.overrides, "checking")?;
        Toggle::of(self.external, self.no_external).apply(&mut config.links.external);
        let stats = crate::engine::Engine::new(config, crate::engine::Mode::Check)?.check(cx.ui)?;
        cx.ui.built(stats.pages, stats.cached);
        Ok(())
    }
}
