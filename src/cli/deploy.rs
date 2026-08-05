//! `baudelaire deploy`: upload the built files to their destinations.

use clap::Args;

use super::{BuildOverrides, Cx, Run, remote};
use crate::error::Result;

/// Arguments for `baudelaire deploy`.
#[derive(Args, Debug, Clone)]
pub struct DeployArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,

    /// Secret for the destination (S3 secret access key, or SSH password/key
    /// passphrase); `-` reads it from stdin. Prefer stdin, the backend's
    /// environment variable, or the interactive prompt: a literal flag can leak
    /// into shell history.
    #[arg(long)]
    pub secret: Option<String>,
    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Report what would change without writing to any destination.
    #[arg(long)]
    pub dry_run: bool,
}

impl remote::Flags for DeployArgs {
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

impl Run for DeployArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "deploying")?;
        remote::run(cx.ui, &config, self, crate::deploy::run)
    }
}
