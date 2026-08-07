//! The typst-html show rules baudelaire replaces.
//!
//! typst-html registers one [`ShowFn`](typst::foundations::ShowFn) per element
//! on the built [`Library`], and `rules.replace` swaps one out. A rule is a bare
//! `fn` with no captured state, so it can only read the element and the style
//! chain: everything configurable is decided here, at install time, by *whether*
//! a rule is installed at all. Adding one is an `impl` in its own module plus one
//! line in [`Rules::install`].
//!
//! A rule reproduces a piece of typst's own output and has no upstream
//! contract; see [`image`] for what it pins.

mod image;

pub use image::MARKER;

use typst::Library;
use typst::foundations::Target;

use crate::config::Config;

/// The rules this build installs over typst-html's own.
pub struct Rules;

impl Rules {
    /// Replace typst-html's rules with baudelaire's, for every one `config`
    /// turns on. Single source of the gates: a rule that is off is not
    /// installed, and typst's own output is what the page gets.
    pub fn install(library: &mut Library, config: &Config) {
        // Externalize typst-embedded images (base64 -> file reference). Off by
        // default and skipped when `html.embed` inlines everything anyway.
        if config.assets.images.externalize(&config.html) {
            library.rules.replace(Target::Html, image::IMAGE_RULE);
        }
    }
}
