//! `assets { }`: the asset pipeline (minify, bundle, fingerprint, images).

pub mod images;
pub mod minify;
pub mod sourcemap;
pub mod tailwind;
pub mod targets;

use std::path::PathBuf;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;
use crate::config::{ImagesConfig, MinifyConfig, SourceMapConfig, TailwindConfig, TargetConfig};

/// Asset pipeline options. All opt-in: a fresh site copies assets verbatim.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct AssetConfig {
    /// What is minified. Its presence turns every kind on; `#false` turns them off again.
    #[key(nested(MinifyConfig))]
    pub minify: MinifyConfig,

    /// The oldest browser versions the stylesheets must run on. Naming any compiles the CSS down to them.
    #[key(nested(TargetConfig))]
    pub targets: TargetConfig,

    /// Bundle JavaScript modules into one file per entry point.
    ///
    /// Required for any JavaScript processing at all.
    #[key(flag)]
    pub bundle: bool,

    /// Put a content hash in each asset's filename, so it can be cached forever.
    ///
    /// `style.css` becomes `style.<hash>.css`, and references are rewritten.
    #[key(flag)]
    pub fingerprint: bool,

    /// What becomes of each kind of asset's source map. Embeds the original sources, so asking for one publishes them.
    #[key(nested(SourceMapConfig))]
    pub sourcemap: SourceMapConfig,

    /// The `tsconfig.json` TypeScript and JSX are transformed against. Unset, one is discovered per script.
    ///
    /// Relative to the project root. Unset, one is discovered per module,
    /// walking up from the file as `tsc` does.
    #[key(opt path)]
    pub tsconfig: Option<PathBuf>,

    /// Image markup and build-time processing.
    #[key(nested(ImagesConfig))]
    pub images: ImagesConfig,

    /// A utility stylesheet generated from the class names the site is written with. Its presence turns it on.
    #[key(nested(TailwindConfig))]
    pub tailwind: TailwindConfig,
}

impl AssetConfig {
    /// Whether JavaScript is actually bundled: configured *and* compiled in.
    pub fn bundling(&self) -> bool {
        self.bundle && cfg!(feature = "js")
    }
}
