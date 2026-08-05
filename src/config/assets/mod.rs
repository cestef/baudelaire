//! `assets { }`: the asset pipeline (minify, bundle, fingerprint, images).

pub mod images;

use std::path::PathBuf;

use crate::config::ImagesConfig;
use crate::config::dispatch::Kind::{Block as Nested, Flag, Path};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// Asset pipeline options. All opt-in: a fresh site copies assets verbatim.
///
/// CSS is minified with lightningcss, independently of bundling. JavaScript is
/// only processed (bundled *and* minified, via rolldown) when
/// [`AssetConfig::bundle`] is set: the bundler owns the whole JS step.
#[derive(Debug, Clone, Hash, Default)]
pub struct AssetConfig {
    /// Minify CSS (lightningcss) and, when bundling, JavaScript (rolldown).
    pub minify: bool,
    /// Bundle JavaScript entry points through rolldown (resolves imports and
    /// tree-shakes). Required for any JavaScript processing.
    pub bundle: bool,
    /// Content-hash asset filenames (`style.css` -> `style.<hash>.css`) and
    /// rewrite references, for far-future caching.
    pub fingerprint: bool,
    /// The `tsconfig.json` the bundler transforms TypeScript and JSX against,
    /// relative to the project root. `None` means the bundler discovers one per
    /// module, walking up from the file as `tsc` does; a path pins the whole
    /// site to one file, wherever the scripts live.
    pub tsconfig: Option<PathBuf>,
    /// Image handling (lazy loading, extraction, optimization, responsive
    /// variants), for both pipeline assets and typst-embedded rasters.
    pub images: ImagesConfig,
}

/// The `assets { .. }` section: the pipeline applied to `paths { assets }`.
impl Section for AssetConfig {
    const RULES: Block<Self> = Block(&[
        ("minify", Flag, "Minify CSS and JavaScript.", |c, n, t| {
            c.minify = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "bundle",
            Flag,
            "Bundle JavaScript modules into one file per entry point.",
            |c, n, t| {
                c.bundle = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "fingerprint",
            Flag,
            "Put a content hash in each asset's filename, so it can be cached forever.",
            |c, n, t| {
                c.fingerprint = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "tsconfig",
            Path,
            "The `tsconfig.json` TypeScript and JSX are transformed against. Unset, one is discovered per script.",
            |c, n, t| {
                c.tsconfig = Some(n.string(t, 0)?.into());
                Ok(())
            },
        ),
        (
            "images",
            Nested(ImagesConfig::rows),
            "Image markup and build-time processing.",
            |c, n, t| c.images.fill(n, t),
        ),
    ]);
}
