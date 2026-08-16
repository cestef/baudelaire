//! The assets a build provides itself: what each one is, when a page gets one,
//! and what goes inside it.

pub mod math;
#[cfg(feature = "tailwind")]
pub mod tailwind;

use std::borrow::Cow;
use std::path::Path;

use crate::config::{Config, MathStyles};
use crate::error::Result;

pub use math::MathSheet;
#[cfg(feature = "tailwind")]
pub use tailwind::TailwindSheet;

/// One asset the build provides itself.
pub trait Owned {
    /// Path relative to the asset root: where the file is written, what a page
    /// links, and the path a site overrides it at by putting its own file
    /// there.
    fn rel<'a>(&self, config: &'a Config) -> &'a Path;

    /// Whether this build serves it at all.
    fn serves(&self, config: &Config) -> bool;

    /// Whether every page links it, or only the pages that ask.
    ///
    /// False means a render pass decides per page, and says so by recording the
    /// name.
    fn everywhere(&self) -> bool {
        true
    }

    /// The bytes to write, borrowed when they are a const and owned when the
    /// site is what they are built from.
    fn bytes(&self, config: &Config) -> Result<Cow<'static, [u8]>>;
}

pub fn builtin() -> Vec<Box<dyn Owned>> {
    vec![
        Box::new(MathSheet),
        #[cfg(feature = "tailwind")]
        Box::new(TailwindSheet),
    ]
}

impl Owned for MathSheet {
    fn rel<'a>(&self, config: &'a Config) -> &'a Path {
        &config.html.math.path
    }

    fn serves(&self, config: &Config) -> bool {
        MathStyles::of(&config.html).served()
    }

    /// Only the pages typst put the block on, which the
    /// [`Math`](crate::render) pass is the only thing that can tell.
    fn everywhere(&self) -> bool {
        false
    }

    fn bytes(&self, _config: &Config) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Borrowed(Self::text().as_bytes()))
    }
}

#[cfg(feature = "tailwind")]
impl Owned for TailwindSheet {
    fn rel<'a>(&self, config: &'a Config) -> &'a Path {
        &config.assets.tailwind.path
    }

    fn serves(&self, config: &Config) -> bool {
        config.assets.tailwind.active()
    }

    fn bytes(&self, config: &Config) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Owned(Self::text(config)?.into_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::builtin;
    use crate::config::Config;

    #[test]
    fn every_owned_asset_is_served_from_its_own_path() {
        let config = Config::default();
        let owned = builtin();
        for (i, asset) in owned.iter().enumerate() {
            assert!(
                !owned[i + 1..]
                    .iter()
                    .any(|other| other.rel(&config) == asset.rel(&config)),
                "`{}` is served twice",
                asset.rel(&config).display()
            );
        }
    }
}
