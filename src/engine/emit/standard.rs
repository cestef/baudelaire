//! Emits the standard.site domain-verification file,
//! `/.well-known/site.standard.publication`, whose body is the publication's
//! `at://` URI.

use super::{Emit, Processor, Site};
use crate::announce::standard::{PUBLICATION, publication_uri};
use crate::config::Config;
use crate::error::Result;

/// Writes `.well-known/site.standard.publication` for the configured `did`.
pub(super) struct WellKnown;

impl WellKnown {
    const DIR: &'static str = ".well-known";
}

impl Processor for WellKnown {
    fn enabled(&self, config: &Config) -> bool {
        config.verify_did(|v| v.wellknown).is_some()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let Some(did) = site.config.verify_did(|v| v.wellknown) else {
            return Ok(());
        };
        let path = site.dist(&[Self::DIR, PUBLICATION.as_str()]);
        out.file(&path, &publication_uri(did).to_string())?;
        out.wrote(&path);
        Ok(())
    }
}
