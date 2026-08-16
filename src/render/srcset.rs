//! The mapping from a source image's request path to its responsive width
//! variants, recorded under the *authored* (pre-fingerprint) URLs.

use std::collections::BTreeMap;

use crate::graph::Hash;

/// One responsive candidate: a variant's authored URL and its intrinsic width,
/// the `srcset` width descriptor (`photo-480.jpg 480w`).
#[derive(Debug, Clone, Hash)]
pub struct Candidate {
    pub url: String,
    pub width: u32,
}

/// The variant-manifest entries a page's images consulted: for each source
/// path probed, a digest of the candidates it found, or `None` when the path
/// named no responsive image.
///
/// The `None` entries matter as much as the rest: an image with no variants
/// today gets some when `responsive` widths change, and the page showing it has
/// to pick up the new `srcset`.
pub type SrcSetDeps = BTreeMap<String, Option<Hash>>;

/// What a source path matched in the manifest, and the entry that decided it.
pub struct Variants<'a> {
    /// The candidates, ascending by width, or `None` for a path that names no
    /// responsive image.
    pub candidates: Option<&'a [Candidate]>,
    /// The manifest entry consulted, with its digest at the time.
    pub probed: SrcSetDeps,
}

/// Maps a source image's authored request path (`/assets/photo.jpg`) to its
/// width candidates, the original included as the largest.
#[derive(Debug, Default, Clone, Hash)]
pub struct SrcSets {
    map: BTreeMap<String, Vec<Candidate>>,
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

    /// The `srcset` candidates for a source path, together with the dependency
    /// that reading them creates.
    pub fn candidates(&self, source: &str) -> Variants<'_> {
        let candidates = self.map.get(source);
        Variants {
            candidates: candidates.map(Vec::as_slice),
            probed: BTreeMap::from([(source.to_owned(), candidates.map(Hash::of))]),
        }
    }

    /// Every source path's candidate digest, for revalidating a page's
    /// recorded [`SrcSetDeps`] against. Digested here rather than in the cache
    /// so the recorded and the current digest cannot be computed two ways.
    pub fn digests(&self) -> BTreeMap<String, Hash> {
        self.map
            .iter()
            .map(|(source, candidates)| (source.clone(), Hash::of(candidates)))
            .collect()
    }
}
