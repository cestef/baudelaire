//! Emits the standard.site domain-verification file.
//!
//! When `publish.standard` carries a `did` and `verify.wellknown` is on, the
//! build writes `/.well-known/site.standard.publication`, whose body is the
//! publication's `at://` URI. An AppView fetches it from the deployed domain to
//! confirm the site owns the publication record. The URI comes from
//! [`crate::announce::standard`], the single source of the record shapes.

use super::{Emit, Processor, Site};
use crate::announce::standard::{PUBLICATION, publication_uri};
use crate::config::Config;
use crate::error::Result;

/// Writes `.well-known/site.standard.publication` for the configured `did`.
pub(super) struct WellKnown;

impl WellKnown {
    /// The standard directory every well-known artifact is served from. Named
    /// once: the emitted path and the progress note are the same location.
    const DIR: &'static str = ".well-known";
}

impl Processor for WellKnown {
    fn enabled(&self, config: &Config) -> bool {
        config.verify_did(|v| v.wellknown).is_some()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        // Same gate as `enabled`, re-read to bind the did (never `None` here).
        let Some(did) = site.config.verify_did(|v| v.wellknown) else {
            return Ok(());
        };
        let path = site.dist(&[Self::DIR, PUBLICATION.as_str()]);
        out.file(&path, &publication_uri(did).to_string())?;
        out.wrote(&path);
        Ok(())
    }
}
