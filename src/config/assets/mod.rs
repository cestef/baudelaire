//! `assets { }`: the asset pipeline (minify, bundle, fingerprint, images).

pub mod images;
pub mod minify;
pub mod sourcemap;
pub mod tailwind;
pub mod targets;

use std::path::PathBuf;

use crate::config::dispatch::Kind::{Block as Nested, Flag, Path};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{ImagesConfig, MinifyConfig, SourceMapConfig, TailwindConfig, TargetConfig};

/// Asset pipeline options. All opt-in: a fresh site copies assets verbatim.
#[derive(Debug, Clone, Hash, Default)]
pub struct AssetConfig {
    pub minify: MinifyConfig,
    /// The oldest browsers the stylesheets must run on.
    pub targets: TargetConfig,
    /// Bundle JavaScript entry points through rolldown. Required for any
    /// JavaScript processing at all.
    pub bundle: bool,
    /// Content-hash asset filenames (`style.css` -> `style.<hash>.css`) and
    /// rewrite references, for far-future caching.
    pub fingerprint: bool,
    /// What becomes of the source map for each kind of processed asset. Off by
    /// default, and has to be: a usable map carries the original sources, so
    /// asking for one publishes them.
    pub sourcemap: SourceMapConfig,
    /// The `tsconfig.json` the bundler transforms TypeScript and JSX against,
    /// relative to the project root. `None` means one is discovered per module,
    /// walking up from the file as `tsc` does.
    pub tsconfig: Option<PathBuf>,
    pub images: ImagesConfig,
    /// The generated utility stylesheet, off unless the block is written.
    pub tailwind: TailwindConfig,
}

impl AssetConfig {
    /// Whether JavaScript is actually bundled: configured *and* compiled in.
    pub fn bundling(&self) -> bool {
        self.bundle && cfg!(feature = "js")
    }
}

impl Section for AssetConfig {
    const RULES: Block<Self> = Block(&[
        (
            "minify",
            Nested(MinifyConfig::rows),
            "What is minified. Its presence turns every kind on; `#false` turns them off again.",
            |c, n, t| c.minify.fill(n, t),
        ),
        (
            "targets",
            Nested(TargetConfig::rows),
            "The oldest browser versions the stylesheets must run on. Naming any compiles the CSS down to them.",
            |c, n, t| c.targets.fill(n, t),
        ),
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
            "sourcemap",
            Nested(SourceMapConfig::rows),
            "What becomes of each kind of asset's source map. Embeds the original sources, so asking for one publishes them.",
            |c, n, t| c.sourcemap.fill(n, t),
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
        (
            "tailwind",
            Nested(TailwindConfig::rows),
            "A utility stylesheet generated from the class names the site is written with. Its presence turns it on.",
            |c, n, t| c.tailwind.fill(n, t),
        ),
    ]);
}
