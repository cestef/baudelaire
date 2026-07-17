//! Materializing externalized images into the asset directory.
//!
//! typst images lifted out of the DOM by [`crate::render`] arrive as
//! [`ImageRef`]s: a served filename plus the source file to copy. Because the
//! asset directory is regenerated every build, they are re-copied whole each
//! time, from fresh pages and cache hits alike. [`Images`] does that copy once
//! per served name, warning when two different sources claim the same name so a
//! silent overwrite never serves the wrong picture.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::Config;
use crate::error::Result;
use crate::error::warning::ImageCollision;
use crate::fs;
use crate::graph::Hash;
use crate::render::ImageRef;
use crate::ui::Ui;

/// A copy run of externalized images into the asset directory, deduped by served
/// name. The first source to claim a name wins; a later source with different
/// bytes is a collision and warns rather than overwriting.
pub(super) struct Images {
    /// The asset directory copies land in.
    dir: PathBuf,
    /// Served name -> (content hash, the source that claimed it), for deduping
    /// identical writes and detecting collisions.
    seen: HashMap<String, (Hash, PathBuf)>,
    count: usize,
    bytes: u64,
}

impl Images {
    pub fn new(config: &Config) -> Self {
        Self {
            dir: config.asset_dist(),
            seen: HashMap::new(),
            count: 0,
            bytes: 0,
        }
    }

    /// Copy every image in `refs`, skipping duplicates and warning on
    /// collisions. Returns `self` so the count and bytes can be read off.
    pub fn copy<'a>(
        mut self,
        refs: impl IntoIterator<Item = &'a ImageRef>,
        ui: &Ui,
    ) -> Result<Self> {
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
                    kept: kept.clone(),
                    dropped: image.source.clone(),
                });
                return Ok(());
            }
            None => {}
        }
        fs::write_all(self.dir.join(&image.name), &data)?;
        self.seen
            .insert(image.name.clone(), (hash, image.source.clone()));
        self.count += 1;
        self.bytes += data.len() as u64;
        Ok(())
    }

    /// Number of files written (duplicates and collisions excluded).
    pub fn count(&self) -> usize {
        self.count
    }

    /// Total bytes written.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}
