//! The generated utility stylesheet: what goes in it, and where it is served
//! from. Class names are read from the site's sources, never from its output.

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
        if !tailwind.preflight {
            utilities.preflight = Preflight::None;
        }
        let sources = Self::sources(config);
        Ok(encre_css::generate(
            sources.iter().map(String::as_str),
            &utilities,
        ))
    }

    /// The text every scanned file holds, skipping the ones that are not UTF-8
    /// rather than failing the build.
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

    /// Whether a file found by walking a named tree is read: everything when
    /// the site named the tree itself, and otherwise only page sources.
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
