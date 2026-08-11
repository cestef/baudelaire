//! The assets a build provides itself: what each one is, when a page gets one,
//! and what goes inside it.
//!
//! A [`Handler`](crate::engine::asset::handler::Handler) claims a *file*; these
//! have none to claim, because their bytes are a const or are built from the
//! site. They still go through the asset pipeline rather than an
//! [`emit`](crate::engine::emit) processor: a processor runs after the pages,
//! and a page can only point at an asset whose fingerprinted name is already in
//! the [`AssetMap`](crate::render::AssetMap).
//!
//! One registry, read from both sides of the build, which is the whole point of
//! this module sitting outside either. The pipeline asks [`Owned::serves`] and
//! [`Owned::bytes`]: what to reserve a name for, and what to write there. The
//! render pass asks [`Owned::rel`] and [`Owned::everywhere`]: what to link from
//! a page's `<head>`, and whether every page gets one or only the ones that ask.
//! Two registries would be two answers to "which assets does this build own",
//! and the linking half was the one written by hand, once per asset.
//!
//! Adding one is an `impl Owned` and one line in [`builtin`]. The filename
//! appears in that impl and nowhere else: no template names it, and the
//! `<link>` is written by one pass for all of them.

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
    ///
    /// Read from the config rather than fixed here, so the name is the site's.
    /// One string answers all three, which is why nothing else may derive it:
    /// two spellings of a generated asset's name is a page linking a file the
    /// build never wrote.
    fn rel<'a>(&self, config: &'a Config) -> &'a Path;

    /// Whether this build serves it at all.
    ///
    /// Answered from the config alone: the pipeline runs before any page has
    /// rendered, so nothing here can know what the pages turned out to contain.
    /// An asset that only some pages need is reserved whenever it is configured
    /// and written only where it was wanted.
    fn serves(&self, config: &Config) -> bool;

    /// Whether every page links it, or only the pages that ask.
    ///
    /// True is the ordinary case: the site turned the asset on, so every page
    /// gets it. False means some render pass decides per page and says so by
    /// recording the name, which is what the equation sheet needs: the pages
    /// with an equation on them are not knowable from the config, and a site
    /// with no math must not ship a stylesheet for it.
    fn everywhere(&self) -> bool {
        true
    }

    /// The bytes to write. Borrowed when they are a const, owned when the site
    /// is what they are built from.
    ///
    /// Fallible because an owned asset is not always a constant: one generated
    /// from the site's own sources can be asked for a config file that is not
    /// there, and a stylesheet quietly reduced to its defaults is a site that
    /// renders wrong out of a green build.
    fn bytes(&self, config: &Config) -> Result<Cow<'static, [u8]>>;
}

/// The owned assets this build knows about.
pub fn builtin() -> Vec<Box<dyn Owned>> {
    vec![
        Box::new(MathSheet),
        #[cfg(feature = "tailwind")]
        Box::new(TailwindSheet),
    ]
}

/// The stylesheet typst's MathML output depends on; see [`MathSheet`] for why
/// the build serves it rather than letting typst inline it per page.
impl Owned for MathSheet {
    fn rel<'a>(&self, config: &'a Config) -> &'a Path {
        &config.html.math.path
    }

    fn serves(&self, config: &Config) -> bool {
        MathStyles::of(&config.html).served()
    }

    /// Only the pages typst put the block on. The
    /// [`Math`](crate::render) pass is what takes that block out, and it is the
    /// only thing that can tell: an equation leaves nothing else behind in the
    /// DOM that says the page had one.
    fn everywhere(&self) -> bool {
        false
    }

    fn bytes(&self, _config: &Config) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Borrowed(Self::text().as_bytes()))
    }
}

/// The utility stylesheet generated from the site's own class names; see
/// [`TailwindSheet`] for why it is generated from the sources rather than from
/// the pages it ends up in.
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

    /// Two assets served from one path would each be written over the other,
    /// and the `<link>` pass would emit the name twice. Checked over the
    /// defaults, which is the one configuration nobody chose.
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
