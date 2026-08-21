//! `assets { images { } }`: markup annotations and build-time optimization.

pub mod optimize;
pub mod responsive;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;
use crate::config::{HtmlConfig, OptimizeConfig, ResponsiveConfig};

/// Image handling: markup annotations and build-time optimization.
#[derive(Debug, Clone, Hash, Table)]
pub struct ImagesConfig {
    /// Mark images `loading="lazy"`.
    ///
    /// Adds `decoding="async"` too.
    #[key(flag)]
    pub lazy: bool,

    /// Write images typst embedded in the page out as their own files.
    ///
    /// Instead of typst's inline base64 `data:` URI. Forced off while
    /// `html.embed` is on, which would re-inline it anyway.
    #[key(flag)]
    pub extract: bool,

    /// Per-format lossless recompression.
    #[key(nested(OptimizeConfig))]
    pub optimize: OptimizeConfig,

    /// Generate width variants and a `srcset`. Its presence turns them on; `#false` turns them off again.
    #[key(nested(ResponsiveConfig))]
    pub responsive: ResponsiveConfig,
}

impl ImagesConfig {
    /// Whether to externalize typst-embedded images: the `extract` switch,
    /// unless `html.embed` is inlining everything.
    pub fn externalize(&self, html: &HtmlConfig) -> bool {
        self.extract && !html.embed
    }
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            lazy: true,
            extract: true,
            optimize: OptimizeConfig::default(),
            responsive: ResponsiveConfig::default(),
        }
    }
}
