//! `assets { images { responsive { } } }`: width variants and `srcset`.

use crate::config::dispatch::Kind::{Number, Numbers, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Responsive images: pre-generate downscaled copies of each raster and let
/// the browser pick the smallest that fits via `srcset`.
///
/// Variants stay in the source format, and a width wider than the source is
/// skipped, never upscaled.
#[derive(Debug, Clone, Hash)]
pub struct ResponsiveConfig {
    pub enabled: bool,
    /// Target widths in CSS pixels. The source's own width is always the
    /// largest candidate, so these only add smaller sizes.
    pub widths: Vec<u32>,
    /// JPEG re-encode quality (`1`–`100`) for downscaled variants. PNG variants
    /// are re-encoded losslessly and ignore this.
    pub quality: u8,
    /// The `sizes` attribute for images the author left unsized, as
    /// `(min-width: 60rem) 640px, 100vw`. `None` emits no attribute, which the
    /// spec treats as `100vw`.
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

impl Section for ResponsiveConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "widths",
            Numbers,
            "The pixel widths to emit a variant at.",
            |c| c.widths.clone().into(),
            |c, n, t| {
                let max_texture_width = 16384;
                c.widths = n.bounds::<u32>(t, 1, max_texture_width)?;
                Ok(())
            },
        ),
        (
            "quality",
            Number,
            "Encoder quality for the generated variants, 1 to 100.",
            |c| c.quality.into(),
            |c, n, t| {
                c.quality = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, 100)?;
                Ok(())
            },
        ),
        (
            "sizes",
            Text,
            "The `sizes` attribute put on every responsive image.",
            |c| c.sizes.clone().into(),
            |c, n, t| {
                c.sizes = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
    ]);
}
