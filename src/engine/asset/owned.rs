//! Assets this crate owns rather than reads, and the registry of them.
//!
//! A [`Handler`](super::Handler) claims a *file*; these have none to claim,
//! because their bytes are a const or are built from the config. They still have
//! to go through the pipeline rather than an [`crate::engine::emit`] processor:
//! a processor runs after the pages, and a page can only point at an asset whose
//! fingerprinted name is already in the [`AssetMap`](crate::render::AssetMap).
//!
//! Adding one is an `impl Owned` and one line in [`builtin`]. What every owned
//! asset shares is not restated per impl: [`super::Assets::generated`] writes
//! them, and the rule that a site's or theme's own file at the same path wins is
//! stated there once.

use std::borrow::Cow;

use crate::config::{Config, MathStyles};
use crate::render::MathSheet;

/// One asset the build provides itself.
pub(super) trait Owned {
    /// Path relative to the asset root, which is also the path a site overrides
    /// it at.
    fn rel(&self) -> &'static str;

    /// Whether this build serves it at all.
    ///
    /// Answered from the config alone: the pipeline runs before any page has
    /// rendered, so nothing here can know what the pages turned out to contain.
    /// An asset that only some pages need is written whenever it is configured
    /// and referenced only where it is wanted.
    fn enabled(&self, config: &Config) -> bool;

    /// The bytes to write. Borrowed when they are a const, owned when the config
    /// is spliced into them.
    fn bytes(&self, config: &Config) -> Cow<'static, [u8]>;
}

/// The owned assets this build knows about.
pub(super) fn builtin() -> Vec<Box<dyn Owned>> {
    vec![Box::new(MathSheet)]
}

/// The stylesheet typst's MathML output depends on; see [`MathSheet`] for why
/// the build serves it rather than letting typst inline it per page.
impl Owned for MathSheet {
    fn rel(&self) -> &'static str {
        Self::REL
    }

    fn enabled(&self, config: &Config) -> bool {
        MathStyles::of(&config.html).served()
    }

    fn bytes(&self, _config: &Config) -> Cow<'static, [u8]> {
        Cow::Borrowed(Self::text().as_bytes())
    }
}
