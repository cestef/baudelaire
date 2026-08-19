//! `artifacts { }`: what a page is drawn as beyond its HTML, each from a paged
//! second compile.

pub mod bundle;
pub mod cards;
pub mod pdf;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::Items;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{BundleConfig, CardsConfig, PdfConfig, Value};

/// Each field is opt-in: a block whose presence turns it on.
#[derive(Debug, Clone, Hash, Default)]
pub struct ArtifactConfig {
    pub cards: CardsConfig,
    pub pdf: PdfConfig,
    /// Documents bound from many pages, keyed by the filename stem every format
    /// of that bundle is written under.
    pub bundles: Vec<(String, BundleConfig)>,
}

impl Section for ArtifactConfig {
    const RULES: Block<Self> = Block(&[
        (
            "cards",
            Nested(CardsConfig::rows),
            "Draw a social card per page. Its presence turns it on; `#false` turns it off again.",
            |c| c.cards.values(),
            |c, n, t| c.cards.fill(n, t),
        ),
        (
            "pdf",
            Nested(PdfConfig::rows),
            "Typeset PDFs beside the pages.",
            |c| c.pdf.values(),
            |c, n, t| c.pdf.fill(n, t),
        ),
        (
            "bundles",
            Items(BundleConfig::rows),
            "One block per bound document, each named by the id its files are written under.",
            |c| Value::each(&c.bundles, Section::values),
            |c, n, t| {
                c.bundles = n.unique(t, "bundle", BundleConfig::item)?;
                Ok(())
            },
        ),
    ]);
}
