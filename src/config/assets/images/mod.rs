//! `assets { images { } }`: markup annotations and build-time optimization.

pub mod optimize;
pub mod responsive;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{HtmlConfig, OptimizeConfig, ResponsiveConfig};

/// Image handling: markup annotations and build-time optimization.
#[derive(Debug, Clone, Hash)]
pub struct ImagesConfig {
    /// Add `loading="lazy"` and `decoding="async"` to `<img>` elements.
    pub lazy: bool,
    /// Write each typst-embedded `image()` out as its own file and reference
    /// it, instead of typst's inline base64 `data:` URI. Forced off while
    /// `html.embed` is on, which would re-inline it anyway.
    pub extract: bool,
    pub optimize: OptimizeConfig,
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

impl Section for ImagesConfig {
    const RULES: Block<Self> = Block(&[
        (
            "lazy",
            Flag,
            "Mark images `loading=\"lazy\"`.",
            |c, n, t| {
                c.lazy = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "extract",
            Flag,
            "Write images typst embedded in the page out as their own files.",
            |c, n, t| {
                c.extract = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "optimize",
            Nested(OptimizeConfig::rows),
            "Per-format lossless recompression.",
            |c, n, t| c.optimize.fill(n, t),
        ),
        (
            "responsive",
            Nested(ResponsiveConfig::rows),
            "Generate width variants and a `srcset`. Its presence turns them on; `#false` turns them off again.",
            |c, n, t| c.responsive.fill(n, t),
        ),
    ]);
}
