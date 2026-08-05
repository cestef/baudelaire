//! `assets { images { optimize { } } }`: per-format recompression.

use crate::config::Named;
use crate::config::dispatch::Kind::{Choice, Line, Number};
use crate::config::dispatch::{Attributed, Attrs, Block, Section};
use crate::config::value::ValueExt;
use crate::mime::ImageFormat;

/// Build-time image optimization, per format. A format is enabled by naming it
/// in the `optimize { .. }` block (`png`, `jpeg`); an absent format is left
/// untouched. Each format carries its own tuning.
#[derive(Debug, Clone, Hash, Default)]
pub struct OptimizeConfig {
    /// PNG optimization (oxipng), when enabled.
    pub png: Option<PngConfig>,
    /// JPEG optimization (re-encode), when enabled.
    pub jpeg: Option<JpegConfig>,
}

impl OptimizeConfig {
    /// Whether any format is enabled.
    pub fn any(&self) -> bool {
        self.png.is_some() || self.jpeg.is_some()
    }

    /// The enabled format for a file extension. `None` when unrecognized or that
    /// format's optimization is off.
    pub fn format(&self, ext: &str) -> Option<ImageFormat> {
        let matched = ImageFormat::from_ext(ext)?;
        let on = match matched {
            ImageFormat::Png => self.png.is_some(),
            ImageFormat::Jpeg => self.jpeg.is_some(),
        };
        on.then_some(matched)
    }
}

/// PNG optimization tuning (oxipng).
#[derive(Debug, Clone, Hash)]
pub struct PngConfig {
    /// Optimization preset, `0` (fast) – `6` (exhaustive).
    pub level: u8,
    /// Which ancillary chunks to strip.
    pub strip: PngStrip,
}

/// PNG ancillary-chunk stripping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PngStrip {
    /// Keep every chunk.
    None,
    /// Strip everything but display-affecting chunks (the default).
    Safe,
    /// Strip all non-critical chunks.
    All,
}

impl Named for PngStrip {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("none", Self::None),
        ("safe", Self::Safe),
        ("all", Self::All),
    ];
}

/// JPEG optimization tuning (re-encode).
#[derive(Debug, Clone, Hash)]
pub struct JpegConfig {
    /// Re-encode quality, `1`–`100`.
    pub quality: u8,
}

impl Default for PngConfig {
    fn default() -> Self {
        // preset 2 balances savings and speed; Safe strips only non-rendering metadata
        Self {
            level: 2,
            strip: PngStrip::Safe,
        }
    }
}

impl Default for JpegConfig {
    fn default() -> Self {
        // 82 is a widely used "visually lossless" default
        Self { quality: 82 }
    }
}

/// The `optimize { png [level=..] [strip=..]; jpeg [quality=..] }` block: each
/// child names a format and enables it, with optional per-format tuning as
/// attributes. Fills onto the existing per-format config so a profile tuning
/// one attribute keeps its siblings.
///
/// One spelling per format. `jpg` used to be a second key onto the same field,
/// so `optimize { jpeg quality=70; jpg quality=90 }` parsed as one format
/// configured twice with the last winning, and the "valid keys" help offered
/// both as if they were different formats.
impl Section for OptimizeConfig {
    const RULES: Block<Self> = Block(&[
        (
            "png",
            Line(PngConfig::rows),
            "Optimize PNGs. Its presence turns them on; the attributes tune it.",
            |c, n, t| c.png.get_or_insert_default().read(n, t),
        ),
        (
            "jpeg",
            Line(JpegConfig::rows),
            "Optimize JPEGs. Its presence turns them on; the attributes tune it.",
            |c, n, t| c.jpeg.get_or_insert_default().read(n, t),
        ),
    ]);
}

impl Attributed for PngConfig {
    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "level",
            Number,
            "Compression effort, 0 to 6. Higher is slower and smaller.",
            |c, v, t, s| {
                c.level = v.bounded(t, s, 0, 6)?;
                Ok(())
            },
        ),
        (
            "strip",
            Choice(PngStrip::names),
            "Which ancillary chunks to discard.",
            |c, v, t, s| {
                c.strip = v.one::<PngStrip>(t, s)?;
                Ok(())
            },
        ),
    ]);
}

impl Attributed for JpegConfig {
    const ATTRS: Attrs<Self> = Attrs(&[(
        "quality",
        Number,
        "Encoder quality, 1 to 100.",
        |c, v, t, s| {
            c.quality = v.bounded(t, s, 1, 100)?;
            Ok(())
        },
    )]);
}
