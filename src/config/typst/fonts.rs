//! `typst { fonts { } }`: where a compile looks for glyphs.

use std::path::PathBuf;

use crate::config::dispatch::Kind::{Flag, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// The font sources a compile may reach.
#[derive(Debug, Clone, Hash)]
pub struct FontConfig {
    /// Directories scanned recursively for font files, relative to the project
    /// root. Searched before the system's own, so a face a site ships wins over
    /// a same-named one that happens to be installed.
    pub paths: Vec<PathBuf>,
    /// Also use the fonts installed on the machine. Off, a build sees only
    /// typst's own bundled faces plus whatever `paths` names.
    pub system: bool,
}

impl FontConfig {
    /// The first declared directory that is not one, resolved against `root`.
    /// A scan of a path that is not there yields no faces and says nothing, so
    /// the caller must turn this into an error before anything compiles.
    pub fn missing(&self, root: &std::path::Path) -> Option<&std::path::Path> {
        self.paths
            .iter()
            .find(|dir| !root.join(dir).is_dir())
            .map(PathBuf::as_path)
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            system: true,
        }
    }
}

impl Section for FontConfig {
    const RULES: Block<Self> = Block(&[
        (
            "paths",
            Texts,
            "Directories scanned recursively for fonts, searched before the system's own.",
            |c| c.paths.clone().into(),
            |c, n, t| {
                c.paths = n.words(t)?.into_iter().map(PathBuf::from).collect();
                Ok(())
            },
        ),
        (
            "system",
            Flag,
            "Also use the fonts installed on the machine. Off, a build sees only what the project ships.",
            |c| c.system.into(),
            |c, n, t| {
                c.system = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
