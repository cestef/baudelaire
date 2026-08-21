//! `artifacts { }`: what a page is drawn as beyond its HTML, each from a paged
//! second compile.

pub mod bundle;
pub mod cards;
pub mod pdf;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;
use crate::config::{BundleConfig, CardsConfig, PdfConfig};

/// Each field is opt-in: a block whose presence turns it on.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct ArtifactConfig {
    /// Draw a social card per page. Its presence turns it on; `#false` turns it off again.
    #[key(nested(CardsConfig))]
    pub cards: CardsConfig,

    /// Typeset PDFs beside the pages.
    #[key(nested(PdfConfig))]
    pub pdf: PdfConfig,

    /// One block per bound document, each named by the id its files are written under.
    ///
    /// Keyed by the filename stem every format of that bundle is written under.
    #[key(items(BundleConfig, "bundle", BundleConfig::item))]
    pub bundles: Vec<(String, BundleConfig)>,
}
