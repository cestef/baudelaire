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

/// A copy run of externalized images into the asset directory, deduped by served
/// name. The first source to claim a name wins; a later source with different
/// bytes is a collision and warns rather than overwriting.
pub(in crate::engine) struct Images {
    /// The asset directory copies land in.
    dir: PathBuf,
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

    /// Copy one image unless another source already claimed its name.
    fn add(&mut self, image: &ImageRef, ui: &Ui) -> Result<()> {
        let data = fs::read(&image.source)?;
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
        // No digest: `integrity` is for the scripts and stylesheets a page
        // loads, and an `<img>` carries none.
        self.emitted
            .insert(format!("{}/{}", self.prefix, image.name), &data, false);
        self.count += 1;
        self.bytes += data.len() as u64;
        Ok(())
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
