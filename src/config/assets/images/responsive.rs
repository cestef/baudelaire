//! `assets { images { responsive { } } }`: width variants and `srcset`.

use crate::config::dispatch::Kind::{Number, Numbers, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Responsive images: pre-generate downscaled copies of each raster and let the
/// browser pick the smallest that fits via `srcset`. Enabled by the presence of
/// a `responsive` block. Variants stay in the source format (a jpeg source
/// yields smaller jpegs); a width wider than the source is skipped, never
/// upscaled.
#[derive(Debug, Clone, Hash)]
pub struct ResponsiveConfig {
    /// Whether to emit width variants.
    pub enabled: bool,
    /// Target widths in CSS pixels. The source's own width is always the largest
    /// candidate, so these only add smaller sizes.
    pub widths: Vec<u32>,
    /// JPEG re-encode quality (`1`–`100`) for downscaled variants. PNG variants
    /// are re-encoded losslessly and ignore this.
    pub quality: u8,
    /// The `sizes` attribute for images the author left unsized: a media-query
    /// list describing the image's displayed width so the browser picks the
    /// smallest variant that fits (`(min-width: 60rem) 640px, 100vw`). `None`
    /// emits no attribute, which the spec treats as `100vw`; set it to the
    /// theme's real content width to stop wide viewports over-fetching. An
    /// authored `sizes` on the image always wins.
    pub sizes: Option<String>,
}

impl ResponsiveConfig {
    /// The widths worth emitting for a source that is `source` pixels wide:
    /// the configured ones below it, deduped and ascending. A width at or above
    /// the source is skipped rather than upscaled, and the source itself is the
    /// largest candidate, so it is not in this list.
    ///
    /// One rule, because two layers apply it and must agree on the answer: the
    /// asset pipeline generates the files, and the render pass names them in a
    /// `srcset` before an extracted image has any.
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
        // opt-in (re-encodes, costs time); the default widths cover phone,
        // tablet, and desktop breakpoints when the block is present but silent.
        Self {
            enabled: false,
            widths: vec![480, 960, 1440],
            quality: 80,
            // no default: the browser already assumes 100vw for w-descriptor
            // srcsets, so emitting it would add bytes for nothing. A theme sets
            // its real content width here.
            sizes: None,
        }
    }
}

/// The `responsive { widths .. ; quality N }` block. Its presence enables
/// width-variant generation; widths and quality keep their defaults unless
/// named.
impl Section for ResponsiveConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "widths",
            Numbers,
            "The pixel widths to emit a variant at.",
            |c, n, t| {
                c.widths = n.widths(t)?;
                Ok(())
            },
        ),
        (
            "quality",
            Number,
            "Encoder quality for the generated variants, 1 to 100.",
            |c, n, t| {
                c.quality = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, 100)?;
                Ok(())
            },
        ),
        (
            "sizes",
            Text,
            "The `sizes` attribute put on every responsive image.",
            |c, n, t| {
                c.sizes = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
    ]);
}
