//! `baudelaire announce`: publish the built site to its announce targets.

use clap::Args;

use super::{BuildOverrides, Cx, Run, remote};
use crate::error::Result;

/// Arguments for `baudelaire announce`.
#[derive(Args, Debug, Clone)]
pub struct AnnounceArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,

    /// Secret (app password / token) for the destination; `-` reads it from
    /// stdin. Prefer stdin, the environment variable, or the interactive prompt:
    /// a literal flag can leak into shell history.
    // Spelled the same as `deploy`'s: one concept, one name. `--password` stays
    // as an alias, since that is what the atproto side calls an app password.
    #[arg(long, alias = "password")]
    pub secret: Option<String>,
    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Report what would change without writing to any destination.
    #[arg(long)]
    pub dry_run: bool,
}

impl remote::Flags for AnnounceArgs {
    fn dry_run(&self) -> bool {
        self.dry_run
    }
    fn yes(&self) -> bool {
        self.yes
    }
    fn secret(&self) -> Option<String> {
        self.secret.clone()
    }
}

impl Run for AnnounceArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "announcing")?;
        remote::run(cx.ui, &config, self, crate::announce::run)
    }
}
