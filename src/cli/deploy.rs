//! `baudelaire deploy`: upload the built files to their destinations.

use clap::Args;

use super::remote::{Destination, PublishArgs};
use super::{BuildOverrides, Cx, Run};
use crate::config::DeployConfig;
use crate::error::{DeployError, Result};

/// Arguments for `baudelaire deploy`.
#[derive(Args, Debug, Clone)]
pub struct DeployArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,

    #[command(flatten)]
    pub publish: PublishArgs,
}

impl DeployArgs {
    /// Where `deploy` sends the site.
    ///
    /// `named` destructures [`DeployConfig`], so a new backend fails to compile
    /// until it is answered for: a pre-flight check that did not know about it
    /// would refuse a config that names it, before the build that would have
    /// worked.
    pub(super) const DESTINATION: Destination = Destination {
        named: |config| {
            let DeployConfig { s3, ssh } = &config.deploy;
            s3.is_some() || ssh.is_some()
        },
        unconfigured: || DeployError::Unconfigured.into(),
        publish: crate::deploy::Deploy::run,
    };
}

impl Run for DeployArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "deploying")?;
        self.publish.send(cx.ui, &config, &Self::DESTINATION)
    }
}
