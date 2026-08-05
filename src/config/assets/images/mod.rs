//! `assets { images { } }`: markup annotations and build-time optimization.

pub mod optimize;
pub mod responsive;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{HtmlConfig, OptimizeConfig, ResponsiveConfig};

/// Image handling: markup annotations and build-time optimization. Grouped so
/// every image setting lives in one `assets { images { .. } }` block.
#[derive(Debug, Clone, Hash)]
pub struct ImagesConfig {
    /// Add `loading="lazy"` and `decoding="async"` to `<img>` elements.
    pub lazy: bool,
    /// Externalize typst-embedded images: write each `image()` to a file under
    /// the asset URL and reference it, instead of typst's default inline
    /// base64 `data:` URI. Forced off while `html.embed` is on (which re-inlines
    /// asset references), so the two never fight.
    pub extract: bool,
    /// Per-format build-time optimization.
    pub optimize: OptimizeConfig,
    /// Responsive width variants (`srcset`).
    pub responsive: ResponsiveConfig,
}

impl ImagesConfig {
    /// Whether to externalize typst-embedded images: the `extract` switch, unless
    /// `html.embed` is inlining everything (in which case externalizing would be
    /// undone immediately).
    pub fn externalize(&self, html: &HtmlConfig) -> bool {
        self.extract && !html.embed
    }
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            // lazy loading is a universal win; optimization is opt-in: it re-encodes and costs time
            lazy: true,
            // externalize by default: smaller HTML and cacheable images, at full
            // sizing parity with typst's inlining. `html.embed` forces inline.
            extract: true,
            optimize: OptimizeConfig::default(),
            responsive: ResponsiveConfig::default(),
        }
    }
}

/// The `images { .. }` section: markup annotations and build-time processing.
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
            "Generate width variants and a `srcset`. Its presence turns them on.",
            |c, n, t| c.responsive.fill(n, t),
        ),
    ]);
}
