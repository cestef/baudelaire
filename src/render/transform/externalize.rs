//! Resolves the `baudelaire:asset:` image markers left by
//! [`crate::world::image_rule::IMAGE_RULE`].
//!
//! The image show rule replaces typst's inline base64 with a marker carrying the
//! source file's project-relative path. This transform rewrites each marked
//! `<img src>` to the URL the file is served at (`/assets/<name>`) and records
//! the `(name, source)` pair so the engine can copy the file into `dist`. Naming
//! follows the asset pipeline: fingerprinted (`photo.<hash>.png`) when
//! `assets { fingerprint }` is on, else the plain filename.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;
use crate::graph::AssetName;

use super::{Cx, DocumentExt, ElementExt, Transform};
use crate::world::image_rule::MARKER;

/// A typst-embedded image lifted out to a file: the filename it is served under
/// (relative to the asset directory) and the source file to copy from. Recorded
/// per page so a cache hit can re-copy the file without recompiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub name: String,
    pub source: PathBuf,
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
                refs.push(image);
                Some(url)
            });
        });
        cx.found.images.extend(refs);
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
            source,
        }
    }
}
