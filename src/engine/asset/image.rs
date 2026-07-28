//! The raster-image handler: optimize a PNG or JPEG through its [`Encoder`],
//! keeping whichever of the original and the result is smaller, and derive
//! responsive width variants for `srcset`.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, imageops::FilterType};

use super::exif::Orientation;
use crate::config::{Config, ImageFormat, JpegConfig, PngConfig, PngStrip};
use crate::error::{AssetError, Result};
use crate::fs;

use super::{Ctx, Handler, PathExt, Variant};

/// Raster images: losslessly optimized (PNG) or re-encoded at a quality (JPEG),
/// and downscaled into responsive width variants. An optimizer must never make a
/// file bigger, so the smaller of the input and output always wins: re-encoding
/// an already-tight file can grow it.
pub(super) struct Raster;

impl Handler for Raster {
    /// Re-encoding an image depends on its own bytes and the `images` options
    /// and nothing else, so the result memoizes across builds and the files
    /// render in parallel.
    fn pure(&self) -> bool {
        true
    }

    fn claims(&self, file: &Path, config: &Config) -> bool {
        // claimed when optimization is on for the format, or responsive variants
        // are wanted for any raster: either way this handler owns the bytes.
        config.assets.images.optimize.format(file.ext()).is_some()
            || (config.assets.images.responsive.enabled
                && ImageFormat::from_ext(file.ext()).is_some())
    }

    fn render(
        &self,
        file: &Path,
        _rel: &Path,
        _map: &super::AssetMap,
        ctx: &Ctx,
    ) -> Result<Option<Vec<u8>>> {
        let bytes = fs::read(file)?;
        // Optimize when configured for this format; otherwise (claimed only for
        // responsive variants) the original is the fallback, copied verbatim.
        let Some(format) = ctx.config.assets.images.optimize.format(file.ext()) else {
            return Ok(Some(bytes));
        };
        let optimized = Self::encoder(format, ctx.config).optimize(&bytes, file)?;
        Ok(Some(if optimized.len() < bytes.len() {
            optimized
        } else {
            bytes
        }))
    }

    fn variants(&self, file: &Path, rel: &Path, ctx: &Ctx) -> Result<Vec<Variant>> {
        let responsive = &ctx.config.assets.images.responsive;
        let Some(format) = ImageFormat::from_ext(file.ext()) else {
            return Ok(Vec::new());
        };
        if !responsive.enabled {
            return Ok(Vec::new());
        }
        let bytes = fs::read(file)?;
        let source = Self::decode(&bytes, format, file)?;
        let full = source.width();
        // Only downscale: a target at or above the source width is skipped, never
        // upscaled. Deduped and sorted so the srcset is tidy and deterministic.
        let mut widths: Vec<u32> = responsive
            .widths
            .iter()
            .copied()
            .filter(|&w| w < full)
            .collect();
        widths.sort_unstable();
        widths.dedup();
        if widths.is_empty() {
            return Ok(Vec::new());
        }
        let mut variants = Vec::with_capacity(widths.len() + 1);
        for width in widths {
            let scaled = source.resize(width, u32::MAX, FilterType::Lanczos3);
            let encoded = Self::encode(&scaled, format, responsive.quality, file)?;
            variants.push(Variant {
                // `photo.jpg` -> `photo-480.jpg`, the same splice a fingerprint
                // uses, so a variant is named like every other asset.
                rel: rel.suffixed(&format!("-{width}")),
                width,
                bytes: Some(encoded),
            });
        }
        // The source itself is the largest candidate; its bytes are render()'s
        // primary output, so it carries none here.
        variants.push(Variant {
            rel: rel.to_path_buf(),
            width: full,
            bytes: None,
        });
        Ok(variants)
    }
}

impl Raster {
    /// The codec for a format, bound to its config. Adding a format is a new
    /// [`Encoder`] impl and an arm here.
    fn encoder(format: ImageFormat, config: &Config) -> Box<dyn Encoder + '_> {
        let optimize = &config.assets.images.optimize;
        match format {
            ImageFormat::Png => Box::new(Png(optimize.png.as_ref().expect("png enabled"))),
            ImageFormat::Jpeg => Box::new(Jpeg(optimize.jpeg.as_ref().expect("jpeg enabled"))),
        }
    }

    /// Decode a raster and bake in its EXIF orientation, so a downscaled JPEG
    /// (which is re-encoded without EXIF) keeps the rotation the source implied.
    fn decode(bytes: &[u8], format: ImageFormat, file: &Path) -> Result<DynamicImage> {
        let decoded =
            image::load_from_memory(bytes).map_err(|e| AssetError::image(file.display(), e))?;
        Ok(match format {
            ImageFormat::Jpeg => Orientation::of_jpeg(bytes).upright(decoded),
            ImageFormat::Png => decoded,
        })
    }

    /// Encode a downscaled variant in its source format: lossy JPEG at `quality`,
    /// lossless PNG (which ignores `quality`).
    fn encode(
        img: &DynamicImage,
        format: ImageFormat,
        quality: u8,
        file: &Path,
    ) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        match format {
            ImageFormat::Jpeg => {
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
                    .encode_image(img)
                    .map_err(|e| AssetError::image(file.display(), e))?
            }
            ImageFormat::Png => img
                .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                .map_err(|e| AssetError::image(file.display(), e))?,
        }
        Ok(out)
    }
}

/// One image codec's optimizer. The caller keeps whichever of the input and the
/// returned bytes is smaller, so an impl may return the input re-encoded freely.
trait Encoder {
    fn optimize(&self, bytes: &[u8], file: &Path) -> Result<Vec<u8>>;
}

/// Lossless PNG recompression with oxipng, stripping chunks per the config.
struct Png<'a>(&'a PngConfig);

impl Encoder for Png<'_> {
    fn optimize(&self, bytes: &[u8], file: &Path) -> Result<Vec<u8>> {
        let mut options = oxipng::Options::from_preset(self.0.level);
        options.strip = match self.0.strip {
            PngStrip::None => oxipng::StripChunks::None,
            PngStrip::Safe => oxipng::StripChunks::Safe,
            PngStrip::All => oxipng::StripChunks::All,
        };
        oxipng::optimize_from_memory(bytes, &options)
            .map_err(|e| AssetError::image(file.display(), e).into())
    }
}

/// Lossy JPEG re-encode at the configured quality. The re-encode strips EXIF,
/// including Orientation, so rotation is baked into the pixels first, or camera
/// photos would come out sideways.
struct Jpeg<'a>(&'a JpegConfig);

impl Encoder for Jpeg<'_> {
    fn optimize(&self, bytes: &[u8], file: &Path) -> Result<Vec<u8>> {
        let decoded =
            image::load_from_memory(bytes).map_err(|e| AssetError::image(file.display(), e))?;
        let decoded = Orientation::of_jpeg(bytes).upright(decoded);
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, self.0.quality)
            .encode_image(&decoded)
            .map_err(|e| AssetError::image(file.display(), e))?;
        Ok(out)
    }
}
