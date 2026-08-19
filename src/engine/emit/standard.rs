//! Emits the standard.site domain-verification file,
//! `/.well-known/site.standard.publication`, whose body is the publication's
//! `at://` URI.

use std::path::PathBuf;

use super::{Emit, Processor, Site};
use crate::announce::standard::PUBLICATION;
use crate::atproto::AtUri;
use crate::config::Config;
use crate::error::Result;

/// Writes `.well-known/site.standard.publication` for the configured `did`.
pub(super) struct WellKnown;

impl WellKnown {
    const DIR: &'static str = ".well-known";
}

impl Processor for WellKnown {
    fn name(&self) -> &'static str {
        "the verification artifact"
    }

    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        vec![Site::at(config, &[Self::DIR, PUBLICATION.as_str()])]
    }

    fn enabled(&self, config: &Config) -> bool {
        config.verify_did(|v| v.wellknown).is_some()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let Some(did) = site.config.verify_did(|v| v.wellknown) else {
            return Ok(());
        };
        let path = site.dist(&[Self::DIR, PUBLICATION.as_str()]);
        out.file(&path, &AtUri::publication(did).to_string())?;
        out.wrote(&path);
        Ok(())
    }
}
