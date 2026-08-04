//! Materializing externalized images into the asset directory.
//!
//! typst images lifted out of the DOM by [`crate::render`] arrive as
//! [`ImageRef`]s: a served filename plus the source file to copy. Because the
//! asset directory is regenerated every build, they are re-copied whole each
//! time, from fresh pages and cache hits alike. [`Images`] does that copy once
//! per served name, warning when two different sources claim the same name so a
//! silent overwrite never serves the wrong picture.

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
/// name, and the downscales to write beside it as `(width, bytes)`. Named
/// because two flavors return it and clippy is right that the tuple was getting
/// hard to read.
type Rendered = (Vec<u8>, Vec<(u32, Vec<u8>)>);

/// A copy run of externalized images into the asset directory, deduped by served
/// name. The first source to claim a name wins; a later source with different
/// bytes is a collision and warns rather than overwriting.
pub(in crate::engine) struct Images {
    /// The asset directory copies land in.
    dir: PathBuf,
    /// The image settings a copy is made under: the optimizer each file goes
    /// through, and the responsive widths cut beside it. Held as the config's
    /// own values rather than re-read per image, and carried in both flavors so
    /// the copy path has one shape: a build with no encoder compiled in reads
    /// them no more than it cuts anything.
    #[cfg_attr(not(feature = "images"), allow(dead_code))]
    settings: crate::config::ImagesConfig,
    /// The cross-build memo the asset pipeline uses, keyed the same way: an
    /// unchanged picture is not re-optimized and its variants are not re-cut on
    /// every build, which a copy that runs on cache hits as well as fresh
    /// renders would otherwise do.
    #[cfg_attr(not(feature = "images"), allow(dead_code))]
    memo: crate::engine::asset::memo::Memo,
    /// The URL prefix those copies are served under, so a copied image can be
    /// weighed by the same name the page that shows it references.
    prefix: String,
    /// The project root, so a diagnostic names `content/a/photo.png` like every
    /// other one rather than `/home/you/mysite/content/a/photo.png`: an
    /// [`ImageRef`] carries an absolute source because the copy needs one.
    root: PathBuf,
    /// Served name -> (content hash, the source that claimed it), for deduping
    /// identical writes and detecting collisions.
    seen: HashMap<String, (Hash, PathBuf)>,
    /// Served URL -> size, for the per-page weight budgets. An externalized
    /// image is the usual way a picture reaches a typst page, so leaving these
    /// out would have made an `images` budget count almost nothing.
    emitted: Emitted,
    count: usize,
    bytes: u64,
}

impl Images {
    pub fn new(config: &Config, root: &Path) -> Self {
        Self {
            dir: config.asset_staging(),
            settings: config.assets.images.clone(),
            memo: crate::engine::asset::memo::Memo::new(config),
            prefix: config.asset_prefix(),
            root: crate::fs::canonical(root),
            seen: HashMap::new(),
            emitted: Emitted::new(config.base_path().to_owned()),
            count: 0,
            bytes: 0,
        }
    }

    /// Copy every image in `refs`, skipping duplicates and warning on
    /// collisions. Returns `self` so the count and bytes can be read off.
    ///
    /// `refs` is sorted first: callers pass fresh pages' images before cached
    /// ones, so "the first source wins" is only a rule once the order is fixed.
    pub fn copy<'a>(
        mut self,
        refs: impl IntoIterator<Item = &'a ImageRef>,
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
    /// the width variants the page that showed it promised.
    ///
    /// Everything the asset pipeline does to a picture in `assets/` is done
    /// here to one lifted out of a page: the same optimizer, the same
    /// downscales, the same memo across builds. The page named the variants
    /// before they existed (`ImageRef::widths`), and this is where they come to
    /// exist; the two agree because both spell a variant's name one way.
    fn add(&mut self, image: &ImageRef, ui: &Ui) -> Result<()> {
        let (data, cut) = self.rendered(image)?;
        let hash = Hash::of_bytes(&data);
        match self.seen.get(&image.name) {
            // identical bytes already written under this name: nothing to do.
            Some((seen, _)) if *seen == hash => return Ok(()),
            // same name, different bytes: keep the first, warn on the rest.
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
        // The asset pipeline regenerates this directory before any image is
        // copied, so a file already here belongs to it. `seen` only dedupes
        // externalized images against each other, so without this an
        // externalized `photo.png` silently replaced a pipeline asset of the
        // same name.
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
            // A pipeline asset of that name wins here as it does above: it was
            // written before any page rendered, and this is the copy.
            if self.dir.join(&name).exists() {
                continue;
            }
            fs::write_all(self.dir.join(&name), &bytes)?;
            self.wrote(&name, &bytes);
        }
        Ok(())
    }

    /// Record a written file: its size for the page weight budgets, and its
    /// share of what the run reports.
    fn wrote(&mut self, name: &str, bytes: &[u8]) {
        // No digest: `integrity` is for the scripts and stylesheets a page
        // loads, and an `<img>` carries none.
        self.emitted
            .insert(format!("{}/{}", self.prefix, name), bytes, false);
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
