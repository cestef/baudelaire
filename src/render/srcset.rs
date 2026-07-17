//! The mapping from a source image's request path to its responsive width
//! variants.
//!
//! Built by the engine's asset pipeline (which generates the downscaled copies)
//! and shared read-only into the render layer, where the [`super::sources`]
//! transform turns each `<img>` that names a mapped source into one carrying a
//! `srcset`. The recorded URLs are the *authored* (pre-fingerprint) paths, so
//! the later [`super::fingerprint`] transform rewrites them alongside every
//! other reference.

use std::collections::BTreeMap;

use crate::graph::{Fingerprint, Hash};

/// One responsive candidate: a variant's authored URL and its intrinsic width,
/// the `srcset` width descriptor (`photo-480.jpg 480w`).
#[derive(Debug, Clone, Hash)]
pub struct Candidate {
    pub url: String,
    pub width: u32,
}

/// Maps a source image's authored request path (`/assets/photo.jpg`) to its
/// width candidates, the original included as the largest. Ordered so its hash
/// is deterministic and folds into the build cache.
#[derive(Debug, Default, Clone, Hash)]
pub struct SrcSets {
    map: BTreeMap<String, Vec<Candidate>>,
}

impl Fingerprint for SrcSets {
    /// The manifest's content digest, folded into the build cache so a page
    /// rebuilds when its images' variants change. Deterministic via the ordered
    /// map.
    fn fingerprint(&self) -> Hash {
        Hash::of(self)
    }
}

impl SrcSets {
    /// Record that `source` (an authored request path) is served at `width` by
    /// the variant at `url` (also an authored path).
    pub fn record(&mut self, source: String, width: u32, url: String) {
        self.map
            .entry(source)
            .or_default()
            .push(Candidate { url, width });
    }

    /// The `srcset` candidates for a source path, ascending by width, or `None`
    /// when the path names no responsive image.
    pub fn candidates(&self, source: &str) -> Option<&[Candidate]> {
        self.map.get(source).map(Vec::as_slice)
    }
}
