//! The typst-html show rules baudelaire replaces, one module each, installed
//! by [`Rules::install`].

mod image;
mod raw;

pub use image::MARKER;
pub(crate) use raw::Grammar;
pub use raw::{LANG, SCOPE, TOKEN, TYPST};

use typst::Library;
use typst::foundations::Target;

use crate::config::Config;

/// The rules this build installs over typst-html's own.
pub struct Rules;

impl Rules {
    /// Replace typst-html's rules with baudelaire's, for every one `config`
    /// turns on; a rule that is off is not installed at all.
    pub fn install(library: &mut Library, config: &Config) {
        if config.assets.images.externalize(&config.html) {
            library.rules.replace(Target::Html, image::IMAGE_RULE);
        }
        if config.html.highlight.enabled {
            library.rules.replace(Target::Html, raw::RAW_RULE);
        }
    }
}
