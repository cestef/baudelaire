//! The raster-image handler: optimize a PNG or JPEG through its [`Encoder`],
//! keeping whichever of the original and the result is smaller, and derive
//! responsive width variants for `srcset`.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, imageops::FilterType};

use super::exif::Orientation;
use crate::config::{Config, JpegConfig, OptimizeConfig, PngConfig, PngStrip, ResponsiveConfig};
use crate::error::{AssetError, Result};
use crate::fs;
use crate::mime::ImageFormat;

use super::{Ctx, Handler, PathExt, Produced, Variant};

/// Raster images: losslessly optimized (PNG) or re-encoded at a quality (JPEG),
/// and downscaled into responsive width variants. An optimizer must never make a
/// file bigger, so the smaller of the input and output always wins: re-encoding
/// an already-tight file can grow it.
pub(in crate::engine) struct Raster;

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
    ) -> Result<Produced> {
        Ok(Produced::bytes(Self::tightened(
            fs::read(file)?,
            file,
            &ctx.config.assets.images.optimize,
        )?))
    }

    fn variants(&self, file: &Path, rel: &Path, ctx: &Ctx) -> Result<Vec<Variant>> {
        let images = &ctx.config.assets.images;
        if !images.responsive.enabled {
            return Ok(Vec::new());
        }
        // Header only: the widths worth cutting are decided from the source's
        // own, and the decode belongs to the cut itself.
        let Ok((full, _)) = image::image_dimensions(file) else {
            return Ok(Vec::new());
        };
        let widths = images.responsive.applicable(full);
        let cut = Self::downscaled(file, &widths, &images.responsive, &images.optimize)?;
        if cut.is_empty() {
            return Ok(Vec::new());
        }
        let mut variants: Vec<Variant> = cut
            .into_iter()
            .map(|(width, bytes)| Variant {
                // `photo.jpg` -> `photo-480.jpg`, the same splice a fingerprint
                // uses, so a variant is named like every other asset.
                rel: rel.suffixed(&format!("-{width}")),
                width,
                bytes: Some(bytes),
            })
            .collect();
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
    /// `widths` cut from `file`, each optimized like the source it came from,
    /// as `(width, bytes)` in the order given. Empty for a file this build
    /// cannot decode, or for no widths at all.
    ///
    /// The one place a downscale is produced, because two callers produce them:
    /// the pipeline, for a file in the asset tree, and the copy that
    /// materializes an image lifted out of a page, whose widths were promised
    /// by that page's `srcset` before these bytes existed.
    pub(in crate::engine) fn downscaled(
        file: &Path,
        widths: &[u32],
        responsive: &ResponsiveConfig,
        optimize: &OptimizeConfig,
    ) -> Result<Vec<(u32, Vec<u8>)>> {
        let Some(format) = ImageFormat::from_ext(file.ext()) else {
            return Ok(Vec::new());
        };
        if widths.is_empty() {
            return Ok(Vec::new());
        }
        let source = Self::decode(&fs::read(file)?, format, file)?;
        widths
            .iter()
            .map(|&width| {
                let scaled = source.resize(width, u32::MAX, FilterType::Lanczos3);
                // Through the optimizer, like the source: an encoder writing a
                // downscale straight out is not competitive with one
                // recompressing it, and a 960px variant heavier than the
                // optimized full-size image made the `srcset` a page offered
                // cost the reader bytes.
                let encoded = Self::encode(&scaled, format, responsive.quality, file)?;
                Ok((width, Self::tightened(encoded, file, optimize)?))
            })
            .collect()
    }

    /// `bytes` through the format's optimizer, keeping whichever of the input
    /// and the result is smaller. The single rule for it: an optimizer must
    /// never make a file bigger, and re-encoding an already-tight one can.
    ///
    /// Unconfigured for the format (the handler is claimed for responsive
    /// variants alone), the bytes pass through as they are.
    pub(in crate::engine) fn tightened(
        bytes: Vec<u8>,
        file: &Path,
        optimize: &OptimizeConfig,
    ) -> Result<Vec<u8>> {
        let Some(format) = optimize.format(file.ext()) else {
            return Ok(bytes);
        };
        let optimized = Self::encoder(format, optimize).optimize(&bytes, file)?;
        Ok(if optimized.len() < bytes.len() {
            optimized
        } else {
            bytes
        })
    }

    /// The codec for a format, bound to its config. Adding a format is a new
    /// [`Encoder`] impl and an arm here.
    fn encoder(format: ImageFormat, optimize: &OptimizeConfig) -> Box<dyn Encoder + '_> {
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
                    .map_err(|e| AssetError::image(file.display(), e))?;
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
