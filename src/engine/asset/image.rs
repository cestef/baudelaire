//! The raster-image handler: optimize a PNG or JPEG through its [`Encoder`],
//! keeping whichever of the original and the result is smaller.

use std::path::Path;

use crate::config::{Config, ImageFormat, JpegConfig, PngConfig, PngStrip};
use crate::error::{AssetError, Result};
use crate::fs;

use super::{Ctx, Handler, PathExt};

/// Raster images: losslessly optimized (PNG) or re-encoded at a quality (JPEG).
/// An optimizer must never make a file bigger, so the smaller of the input and
/// output always wins — re-encoding an already-tight file can grow it.
pub(super) struct Raster;

impl Handler for Raster {
    fn claims(&self, file: &Path, config: &Config) -> bool {
        config.images.optimize.format(file.ext()).is_some()
    }

    fn render(
        &self,
        file: &Path,
        _rel: &Path,
        _map: &super::AssetMap,
        ctx: &Ctx,
    ) -> Result<Option<Vec<u8>>> {
        let format = ctx
            .config
            .images
            .optimize
            .format(file.ext())
            .expect("claimed only formats it optimizes");
        let bytes = fs::read(file)?;
        let optimized = Self::encoder(format, ctx.config).optimize(&bytes, file)?;
        Ok(Some(if optimized.len() < bytes.len() {
            optimized
        } else {
            bytes
        }))
    }
}

impl Raster {
    /// The codec for a format, bound to its config. Adding a format is a new
    /// [`Encoder`] impl and an arm here.
    fn encoder(format: ImageFormat, config: &Config) -> Box<dyn Encoder + '_> {
        let optimize = &config.images.optimize;
        match format {
            ImageFormat::Png => Box::new(Png(optimize.png.as_ref().expect("png enabled"))),
            ImageFormat::Jpeg => Box::new(Jpeg(optimize.jpeg.as_ref().expect("jpeg enabled"))),
        }
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

/// Lossy JPEG re-encode at the configured quality. The re-encode strips EXIF —
/// including Orientation — so rotation is baked into the pixels first, or camera
/// photos would come out sideways.
struct Jpeg<'a>(&'a JpegConfig);

impl Encoder for Jpeg<'_> {
    fn optimize(&self, bytes: &[u8], file: &Path) -> Result<Vec<u8>> {
        let decoded =
            image::load_from_memory(bytes).map_err(|e| AssetError::image(file.display(), e))?;
        let decoded = crate::engine::exif::Orientation::of_jpeg(bytes).upright(decoded);
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, self.0.quality)
            .encode_image(&decoded)
            .map_err(|e| AssetError::image(file.display(), e))?;
        Ok(out)
    }
}
