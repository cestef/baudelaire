//! Resolves the `baudelaire:asset:` image markers left by
//! [`crate::world::image_rule::IMAGE_RULE`].
//!
//! The image show rule replaces typst's inline base64 with a marker carrying the
//! source file's project-relative path. This transform rewrites each marked
//! `<img src>` to the URL the file is served at (`/assets/<name>`) and records
//! the `(name, source)` pair so the engine can copy the file into `dist`. Naming
//! follows the asset pipeline: fingerprinted (`photo.<hash>.png`) when
//! `assets { fingerprint }` is on, else the plain filename.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;
use crate::graph::AssetName;
use crate::render::Candidate;

use super::{Cx, DocumentExt, ElementExt, Transform};
use crate::world::image_rule::MARKER;

/// A typst-embedded image lifted out to a file: the filename it is served under
/// (relative to the asset directory), the source file to copy from, and the
/// widths the page's `srcset` promised. Recorded per page so a cache hit can
/// re-copy the file, and re-cut its variants, without recompiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub name: String,
    pub source: PathBuf,
    /// Downscaled widths to write beside it, ascending, empty when the site
    /// asks for no variants or the source is too small to have any. The names
    /// are not carried: they are this one's, spliced per width, and a second
    /// list of them is a second chance to disagree with the page.
    #[serde(default)]
    pub widths: Vec<u32>,
}

/// The [`Transform`] that turns image markers into served asset references.
pub(super) struct Externalize;

impl Transform for Externalize {
    fn enabled(&self, config: &Config) -> bool {
        config.assets.images.externalize(&config.html)
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let config = cx.config;
        let root = cx.root;
        // Gather markers while walking, then record: the walk borrows the DOM
        // mutably, so the `cx` accumulator is written after it finishes.
        let mut refs = Vec::new();
        let mut variants = BTreeMap::new();
        doc.walk(|element| {
            if element.tag != tag::img {
                return;
            }
            element.rewrite(&[attr::src], |src| {
                let vpath = src.strip_prefix(MARKER)?;
                // A picture the asset pipeline already owns is referenced where
                // the pipeline put it, not copied a second time: extracting it
                // wrote the source bytes over (or beside) the processed ones,
                // warned that two images claimed one name, and left the page
                // pointing at whichever won.
                if let Some(url) = ImageRef::pipelined(vpath, config) {
                    return Some(url);
                }
                let image = ImageRef::of(vpath, root, config);
                let url = format!("/{}/{}", config.asset_name(), image.name);
                // The variants this image is about to be given, named before
                // they exist: the copy pass cuts exactly these widths, and the
                // `srcset` writer reads them from here as it reads the
                // pipeline's manifest for an image in the asset tree.
                if !image.widths.is_empty() {
                    variants.insert(url.clone(), image.candidates(&url, config));
                }
                refs.push(image);
                Some(url)
            });
        });
        cx.found.images.extend(refs);
        cx.extracted.extend(variants);
    }
}

impl ImageRef {
    /// The URL an image *inside the asset tree* is already served at, or `None`
    /// for one anywhere else.
    ///
    /// The pipeline reads that tree, so such a file has been optimized, given
    /// its responsive variants and (with `fingerprint`) renamed, and is on disk
    /// under the URL its own relative path spells. The authored URL is what is
    /// emitted, so the `srcset` and fingerprint transforms match it exactly as
    /// they match a `src` written by hand.
    fn pipelined(vpath: &str, config: &Config) -> Option<String> {
        let rel = Path::new(vpath).strip_prefix(&config.paths.assets).ok()?;
        Some(config.asset_url(rel))
    }

    /// The reference for a marker's virtual path. The name is fingerprinted
    /// (content hash spliced in) when asset fingerprinting is on, so
    /// externalized images cache far-future like every other asset; otherwise
    /// the plain filename is kept. A hash read that fails falls back to the
    /// plain name; the engine's copy then surfaces the unreadable source.
    ///
    /// [`AssetName::file`] rather than [`AssetName::path`]: the image keeps only
    /// its base name, since it is served flat out of the asset root whatever
    /// content subdirectory it was authored in. The splice itself is the asset
    /// pipeline's, so the two cannot drift.
    fn of(vpath: &str, root: &Path, config: &Config) -> Self {
        let source = root.join(vpath);
        let digest = config
            .assets
            .fingerprint
            .then(|| crate::fs::read(&source).ok())
            .flatten()
            .map(|bytes| AssetName::digest(&bytes));
        Self {
            name: AssetName::new(Path::new(vpath), digest).file(),
            widths: Self::widths(&source, config),
            source,
        }
    }

    /// The widths a `srcset` for this image can offer: the configured ones
    /// below the source's own, read from the file's header rather than by
    /// decoding it. Empty when the site asks for no variants, when the flavor
    /// has no encoder to cut them with, or when the file is not a raster this
    /// build can read.
    #[cfg(feature = "images")]
    fn widths(source: &Path, config: &Config) -> Vec<u32> {
        let responsive = &config.assets.images.responsive;
        if !responsive.enabled {
            return Vec::new();
        }
        // Header only: the copy pass decodes, and this runs for every image on
        // every page that shows one.
        let Ok((width, _)) = image::image_dimensions(source) else {
            return Vec::new();
        };
        responsive.applicable(width)
    }

    /// No encoder, no variants: a slim build copies rasters through, and a
    /// `srcset` naming files it will not cut is a page of dead candidates.
    #[cfg(not(feature = "images"))]
    fn widths(_source: &Path, _config: &Config) -> Vec<u32> {
        Vec::new()
    }

    /// This image's `srcset` candidates: one per width, named by splicing the
    /// width into the served name the way the pipeline splices it into an
    /// asset's, plus the source itself as the largest.
    ///
    /// The names are derived here and again where the files are written, from
    /// this one rule ([`Self::variant`]), because the page is served before the
    /// bytes are cut.
    fn candidates(&self, url: &str, config: &Config) -> Vec<Candidate> {
        let dir = url.rsplit_once('/').map_or("", |(dir, _)| dir);
        self.widths
            .iter()
            .map(|&width| Candidate {
                url: format!("{dir}/{}", Self::variant(&self.name, width)),
                width,
            })
            .chain(self.source_candidate(url, config))
            .collect()
    }

    /// The source itself, the largest candidate, with the intrinsic width a
    /// browser needs to choose between it and the downscales.
    #[cfg(feature = "images")]
    fn source_candidate(&self, url: &str, _config: &Config) -> Option<Candidate> {
        let (width, _) = image::image_dimensions(&self.source).ok()?;
        Some(Candidate {
            url: url.to_owned(),
            width,
        })
    }

    /// The signature mirrors the `images`-on one, which is why it takes a
    /// `self` it has nothing to read: with no encoder there are no variants to
    /// be the largest of.
    #[cfg(not(feature = "images"))]
    #[allow(clippy::unused_self)]
    fn source_candidate(&self, _url: &str, _config: &Config) -> Option<Candidate> {
        None
    }

    /// The served name of one width variant: `photo-480.png`, or
    /// `photo-480.<digest>.png` where the name it is cut from is fingerprinted.
    ///
    /// The digest is the *source's*, carried over from the primary rather than
    /// taken over the variant's own bytes: the two change together, and the
    /// page names the file before those bytes exist.
    pub fn variant(name: &str, width: u32) -> String {
        let (stem, digest) = match name.split_once('.') {
            Some((stem, rest)) => (stem, format!(".{rest}")),
            None => (name, String::new()),
        };
        format!("{stem}-{width}{digest}")
    }
}
