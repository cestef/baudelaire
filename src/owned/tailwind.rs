//! The generated utility stylesheet: what goes in it, and where it is served
//! from.
//!
//! What is read is the *source*, not the output: a class name is written in a
//! template or a page, and a build that waited for the rendered HTML could not
//! name the sheet in the `<head>` of the pages it was still generating from.
//! That is also how Tailwind itself works, and why its config lists content
//! globs rather than a build directory.
//!
//! Nothing outside this module spells the filename. A template does not link
//! the sheet: [`Sheets`](crate::render) does, on every page, because a utility
//! class is written in whatever template happens to use it and a page that got
//! the classes without the rules is a page that renders wrong.

use std::path::{Path, PathBuf};

use encre_css::Config as Utilities;
use encre_css::preflight::Preflight;

use crate::config::Config;
use crate::error::{AssetError, Result};
use crate::fs;

/// The utility stylesheet this build generates.
pub struct TailwindSheet;

impl TailwindSheet {
    /// Generate the sheet from everything the site's `scan` names.
    pub(super) fn text(config: &Config) -> Result<String> {
        let tailwind = &config.assets.tailwind;
        let mut utilities = match &tailwind.config {
            Some(path) => {
                let path = config.root.join(path);
                Utilities::from_file(&path).map_err(|e| AssetError::tailwind(path.display(), e))?
            }
            None => Utilities::default(),
        };
        // Only ever read to turn one off; see the config field. A `config` file
        // that states its own preflight keeps it, because it said so and this
        // did not.
        if !tailwind.preflight {
            utilities.preflight = Preflight::None;
        }
        let sources = Self::sources(config);
        Ok(encre_css::generate(
            sources.iter().map(String::as_str),
            &utilities,
        ))
    }

    /// The text every scanned file holds.
    ///
    /// A file that is not UTF-8 is skipped rather than failing the build: a
    /// named tree is a tree, and a colocated photograph sitting in one is not a
    /// mistake the author has to hear about. Nothing here can fail for the same
    /// reason: an unreadable file yields no class names, and the page that
    /// needed it will say so on its own account.
    fn sources(config: &Config) -> Vec<String> {
        Self::scanned(config)
            .iter()
            .flat_map(|path| Self::read(path, config))
            .collect()
    }

    /// The files one `scan` entry names: itself, if it is a file, and otherwise
    /// everything under it.
    fn read(path: &Path, config: &Config) -> Vec<String> {
        if path.is_file() {
            return fs::read_to_string(path).ok().into_iter().collect();
        }
        fs::Walk::new(path)
            .files()
            .unwrap_or_default()
            .into_iter()
            .filter(|file| Self::wanted(file, config))
            .filter_map(|file| fs::read_to_string(&file).ok())
            .collect()
    }

    /// Whether a file found by walking a named tree is read.
    ///
    /// Everything, when the site named the tree itself: it said what it meant.
    /// The default trees are the content and template ones, where only the two
    /// languages a page is written in can carry a class name, and where a
    /// colocated image would otherwise be read whole on every build.
    fn wanted(file: &Path, config: &Config) -> bool {
        if !config.assets.tailwind.scan.is_empty() {
            return true;
        }
        [Config::TYPST, Config::MARKDOWN]
            .iter()
            .any(|ext| Config::has_ext(file, ext))
    }

    /// The trees and files to read, as the site named them, or the content and
    /// template trees when it named none.
    fn scanned(config: &Config) -> Vec<PathBuf> {
        let scan = &config.assets.tailwind.scan;
        if scan.is_empty() {
            return vec![config.paths.content.clone(), config.paths.templates.clone()];
        }
        scan.iter().map(|path| config.root.join(path)).collect()
    }
}
