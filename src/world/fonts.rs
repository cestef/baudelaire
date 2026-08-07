//! The fonts a compile may reach, and the order they are searched in.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst_kit::fonts::FontStore;

use crate::config::FontConfig;

/// The initializer a [`Fonts`] defers. Boxed because it captures the site's own
/// directories, which a plain `fn()` cannot.
type Discover = Box<dyn FnOnce() -> FontStore + Send + Sync>;

/// Every face a compile can resolve a glyph to, discovered on first lookup.
///
/// Lazy because scanning font directories and parsing fontconfig is not cheap
/// and a fully-cached rebuild compiles nothing: a site whose pages all hit the
/// cache never pays for a single directory walk.
pub(super) struct Fonts(LazyLock<FontStore, Discover>);

impl Fonts {
    /// The store this site asks for, resolved against `root`, with nothing
    /// scanned yet.
    ///
    /// Search order is the whole of what this decides, and it runs from most
    /// specific to least: typst's own bundled faces, then the directories the
    /// site ships, then the machine's.
    ///
    /// The bundled faces lead so a glyph resolves the way it does under `typst`
    /// itself rather than falling back to whatever the system offers (which can
    /// rasterize digits as colour-font images). A site's own directories come
    /// next, ahead of the machine's, because shipping a face is how a site says
    /// it means *that* one and not a same-named installed version of it.
    pub(super) fn of(config: &FontConfig, root: &Path) -> Self {
        let paths: Vec<PathBuf> = config.paths.iter().map(|dir| root.join(dir)).collect();
        let system = config.system;
        Self(LazyLock::new(Box::new(move || {
            let mut fonts = FontStore::new();
            // Without the `embedded-fonts` feature the defaults are not bundled,
            // so resolution depends entirely on the other two sources.
            #[cfg(feature = "embedded-fonts")]
            fonts.extend(typst_kit::fonts::embedded());
            for dir in &paths {
                fonts.extend(typst_kit::fonts::scan(dir));
            }
            if system {
                fonts.extend(typst_kit::fonts::system());
            }
            fonts
        })))
    }

    pub(super) fn book(&self) -> &LazyHash<FontBook> {
        self.0.book()
    }

    pub(super) fn font(&self, index: usize) -> Option<Font> {
        self.0.font(index)
    }
}
