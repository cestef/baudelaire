//! `assets { images { optimize { } } }`: per-format recompression.

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::Value;
use crate::config::dispatch::Kind::Line;
use crate::config::dispatch::{Attributed, Attrs, Block, Section};
use crate::config::vocab::{attr, rule};
use crate::mime::ImageFormat;

/// Build-time image optimization, per format. A format is enabled by naming it
/// in the `optimize { .. }` block; `None` leaves that format untouched.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct OptimizeConfig {
    /// Optimize PNGs. Its presence turns them on; the attributes tune it.
    #[key(custom(
        Line(PngConfig::rows),
        |c: &Self| c.png.as_ref().map_or(Value::Unset, Attributed::values),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| c.png.get_or_insert_default().read(n, t),
    ))]
    pub png: Option<PngConfig>,

    /// Optimize JPEGs. Its presence turns them on; the attributes tune it.
    #[key(custom(
        Line(JpegConfig::rows),
        |c: &Self| c.jpeg.as_ref().map_or(Value::Unset, Attributed::values),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| c.jpeg.get_or_insert_default().read(n, t),
    ))]
    pub jpeg: Option<JpegConfig>,
}

impl OptimizeConfig {
    pub fn any(&self) -> bool {
        self.png.is_some() || self.jpeg.is_some()
    }

    /// The enabled format for a file extension. `None` when unrecognized or
    /// that format's optimization is off.
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
#[derive(Debug, Clone, Hash, Table)]
#[table(impl = Attributed, const ATTRS: Attrs<Self> = Attrs, rule = attr)]
pub struct PngConfig {
    /// Compression effort, 0 to 6. Higher is slower and smaller.
    #[key(bounded(u8, 0, 6))]
    pub level: u8,

    /// Which ancillary chunks to discard.
    #[key(choice(PngStrip))]
    pub strip: PngStrip,
}

/// PNG ancillary-chunk stripping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PngStrip {
    /// Keep every chunk.
    None,
    /// Strip everything but display-affecting chunks.
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
#[derive(Debug, Clone, Hash, Table)]
#[table(impl = Attributed, const ATTRS: Attrs<Self> = Attrs, rule = attr)]
pub struct JpegConfig {
    /// Encoder quality, 1 to 100.
    #[key(bounded(u8, 1, 100))]
    pub quality: u8,
}

impl Default for PngConfig {
    fn default() -> Self {
        Self {
            level: 2,
            strip: PngStrip::Safe,
        }
    }
}

impl Default for JpegConfig {
    fn default() -> Self {
        Self { quality: 82 }
    }
}
