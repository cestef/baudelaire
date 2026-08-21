//! `assets { images { responsive { } } }`: width variants and `srcset`.

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Numbers;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;

/// Responsive images: pre-generate downscaled copies of each raster and let
/// the browser pick the smallest that fits via `srcset`.
///
/// Variants stay in the source format, and a width wider than the source is
/// skipped, never upscaled.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct ResponsiveConfig {
    pub enabled: bool,

    /// The pixel widths to emit a variant at.
    ///
    /// The source's own width is always the largest candidate, so these only
    /// add smaller sizes.
    #[key(custom(
        Numbers,
        |c: &Self| c.widths.clone().into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let max_texture_width = 16384;
            c.widths = n.bounds::<u32>(t, 1, max_texture_width)?;
            Ok(())
        },
    ))]
    pub widths: Vec<u32>,

    /// Encoder quality for the generated variants, 1 to 100.
    ///
    /// JPEG only: PNG variants are re-encoded losslessly and ignore this.
    #[key(bounded(u8, 1, 100))]
    pub quality: u8,

    /// The `sizes` attribute put on every responsive image.
    ///
    /// As `(min-width: 60rem) 640px, 100vw`. `None` emits no attribute, which
    /// the spec treats as `100vw`.
    #[key(opt text)]
    pub sizes: Option<String>,
}

impl ResponsiveConfig {
    /// The widths worth emitting for a source that is `source` pixels wide: the
    /// configured ones below it, deduped and ascending. The source itself is
    /// the largest candidate, so it is not in this list.
    pub fn applicable(&self, source: u32) -> Vec<u32> {
        let mut widths: Vec<u32> = self
            .widths
            .iter()
            .copied()
            .filter(|&w| w < source)
            .collect();
        widths.sort_unstable();
        widths.dedup();
        widths
    }
}

impl Default for ResponsiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            widths: vec![480, 960, 1440],
            quality: 80,
            sizes: None,
        }
    }
}
