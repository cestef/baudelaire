//! Materializing externalized images into the asset directory.
//!
//! The asset directory is regenerated every build, so images lifted out of the
//! DOM by [`crate::render`] are re-copied whole each time, from fresh pages and
//! cache hits alike.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::Result;
use crate::error::warning::ImageCollision;
use crate::fs;
use crate::graph::Hash;
use crate::render::{Emitted, ImageRef};
use crate::ui::Ui;

/// One extracted image's outputs: the primary bytes to write under its served
/// name, and the downscales to write beside it as `(width, bytes)`.
type Rendered = (Vec<u8>, Vec<(u32, Vec<u8>)>);

/// A copy run of externalized images into the asset directory, deduped by served
/// name. The first source to claim a name wins; a later source with different
/// bytes is a collision and warns rather than overwriting.
pub(in crate::engine) struct Images<'a> {
    /// The site, so a written file is weighed under exactly the URL
    /// [`Config::asset_url`] gives the page that shows it.
    config: &'a Config,
    dir: PathBuf,
    /// The optimizer each file goes through and the responsive widths cut
    /// beside it. Carried in both flavors so the copy path has one shape.
    #[cfg_attr(not(feature = "images"), allow(dead_code))]
    settings: crate::config::ImagesConfig,
    /// The cross-build memo the asset pipeline uses, keyed the same way, so an
    /// unchanged picture is not re-optimized on every build.
    #[cfg_attr(not(feature = "images"), allow(dead_code))]
    memo: crate::engine::asset::memo::Memo,
    /// The project root, so a diagnostic names `content/a/photo.png` rather
    /// than the absolute source an [`ImageRef`] carries.
    root: PathBuf,
    /// Served name -> (content hash, the source that claimed it).
    seen: HashMap<String, (Hash, PathBuf)>,
    /// Served URL -> size, for the per-page weight budgets.
    emitted: Emitted,
    count: usize,
    bytes: u64,
}

impl<'a> Images<'a> {
    pub fn new(config: &'a Config, root: &Path) -> Self {
        Self {
            config,
            dir: config.asset_staging(),
            settings: config.assets.images.clone(),
            memo: crate::engine::asset::memo::Memo::new(config),
            root: crate::fs::canonical(root),
            seen: HashMap::new(),
            emitted: Emitted::new(config.base_path().to_owned()),
            count: 0,
            bytes: 0,
        }
    }

    /// Copy every image in `refs`, skipping duplicates and warning on
    /// collisions. `refs` is sorted first, since "the first source wins" is
    /// only a rule once the order no longer depends on which pages were cached.
    pub fn copy<'r>(
        mut self,
        refs: impl IntoIterator<Item = &'r ImageRef>,
        ui: &Ui,
    ) -> Result<Self> {
        let mut refs: Vec<&ImageRef> = refs.into_iter().collect();
        refs.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.source.cmp(&b.source)));
        for image in refs {
            self.add(image, ui)?;
        }
        Ok(self)
    }

    /// Copy one image unless another source already claimed its name, and cut
    /// the width variants the page that showed it promised
    /// ([`ImageRef::widths`]). A file already in the asset directory belongs to
    /// the asset pipeline, which regenerates it before any image is copied, and
    /// wins over an externalized image of the same name.
    fn add(&mut self, image: &ImageRef, ui: &Ui) -> Result<()> {
        let (data, cut) = self.rendered(image)?;
        let hash = Hash::of_bytes(&data);
        match self.seen.get(&image.name) {
            Some((seen, _)) if *seen == hash => return Ok(()),
            Some((_, kept)) => {
                ui.warn(ImageCollision {
                    name: image.name.clone(),
                    kept: self.relative(kept),
                    dropped: self.relative(&image.source),
                });
                return Ok(());
            }
            None => {}
        }
        let dst = self.dir.join(&image.name);
        if dst.exists() {
            ui.warn(ImageCollision {
                name: image.name.clone(),
                kept: self.relative(&dst),
                dropped: self.relative(&image.source),
            });
            return Ok(());
        }
        fs::write_all(&dst, &data)?;
        self.seen
            .insert(image.name.clone(), (hash, image.source.clone()));
        self.wrote(&image.name, &data);
        for (width, bytes) in cut {
            let name = ImageRef::variant(&image.name, width);
            if self.dir.join(&name).exists() {
                continue;
            }
            fs::write_all(self.dir.join(&name), &bytes)?;
            self.wrote(&name, &bytes);
        }
        Ok(())
    }

    /// Record a written file: its size for the page weight budgets, and its
    /// share of what the run reports. No digest, since an `<img>` carries no
    /// `integrity`.
    fn wrote(&mut self, name: &str, bytes: &[u8]) {
        self.emitted
            .insert(self.config.asset_url(Path::new(name)), bytes, false);
        self.count += 1;
        self.bytes += bytes.len() as u64;
    }

    /// What this image renders to: the optimized primary bytes and the widths
    /// cut beside it, from the memo when nothing that shapes them has changed.
    #[cfg(feature = "images")]
    fn rendered(&self, image: &ImageRef) -> Result<Rendered> {
        use crate::engine::asset::image::Raster;

        let source = fs::read(&image.source)?;
        let key = self.memo.key(&source, Path::new(&image.name));
        if let Some((Some(primary), variants)) = self.memo.get(&key) {
            let cut = variants
                .into_iter()
                .filter_map(|variant| Some((variant.width, variant.bytes?)))
                .collect();
            return Ok((primary, cut));
        }
        let primary = Raster::tightened(source, &image.source, &self.settings.optimize)?;
        let cut = Raster::downscaled(
            &image.source,
            &image.widths,
            &self.settings.responsive,
            &self.settings.optimize,
        )?;
        self.memo.put(
            &key,
            Some(&primary),
            &cut.iter()
                .map(|(width, bytes)| crate::engine::asset::handler::Variant {
                    rel: PathBuf::from(ImageRef::variant(&image.name, *width)),
                    width: *width,
                    bytes: Some(bytes.clone()),
                })
                .collect::<Vec<_>>(),
        );
        Ok((primary, cut))
    }

    /// No encoder compiled in: the file is copied as it is, and the page that
    /// showed it promised no variants ([`ImageRef::widths`] is empty there).
    /// The signature mirrors the `images`-on one, which is why it takes a
    /// `self` it has nothing to read.
    #[cfg(not(feature = "images"))]
    #[allow(clippy::unused_self)]
    fn rendered(&self, image: &ImageRef) -> Result<Rendered> {
        Ok((fs::read(&image.source)?, Vec::new()))
    }

    /// A path as diagnostics spell it: relative to the project root when it
    /// lies inside, unchanged otherwise.
    fn relative(&self, path: &Path) -> PathBuf {
        path.strip_prefix(&self.root).unwrap_or(path).to_path_buf()
    }

    /// Number of files written (duplicates and collisions excluded).
    pub fn count(&self) -> usize {
        self.count
    }

    /// Total bytes written.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// What was copied and how large each one is, to be weighed alongside what
    /// the asset pipeline emitted.
    pub fn emitted(&self) -> &Emitted {
        &self.emitted
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Images;
    use crate::config::Config;

    /// The weight ledger is keyed by URL, and the page asks for the URL
    /// `Config::asset_url` gives it. A copied image's served name is built from
    /// a path, so on a host whose separator is not `/` the two spellings used
    /// to differ and every externalized image weighed nothing.
    #[test]
    fn a_nested_image_is_weighed_under_the_url_the_page_asks_for() {
        let config = Config::default();
        let name = Path::new("sub")
            .join("photo.png")
            .to_string_lossy()
            .into_owned();
        let mut images = Images::new(&config, Path::new("."));
        images.wrote(&name, b"xy");
        let asked = config.asset_url(Path::new(&name));
        assert_eq!(
            images.emitted().at(&asked).map(|e| e.bytes),
            Some(2),
            "weighed under a name the page never asks for: {asked}"
        );
    }
}
