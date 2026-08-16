//! `baudelaire announce`: publish the built site to its announce targets.

use clap::Args;

use super::remote::{Destination, PublishArgs};
use super::{BuildOverrides, Cx, Run};
use crate::config::AnnounceConfig;
use crate::error::{AnnounceError, Result};

#[derive(Args, Debug, Clone)]
pub struct AnnounceArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,

    #[command(flatten)]
    pub publish: PublishArgs,
}

impl AnnounceArgs {
    /// Where `announce` sends the site's metadata; `named` destructures
    /// [`AnnounceConfig`] so a new backend fails to compile until it is
    /// answered for here.
    pub(super) const DESTINATION: Destination = Destination {
        named: |config| {
            let AnnounceConfig { standard } = &config.announce;
            standard.is_some()
        },
        unconfigured: || AnnounceError::Unconfigured.into(),
        publish: crate::announce::Announce::run,
    };
}

impl Run for AnnounceArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "announcing")?;
        self.publish.send(cx.ui, &config, &Self::DESTINATION)
    }
}
