//! `typst { fonts { } }`: where a compile looks for glyphs.

use std::path::PathBuf;

use crate::config::dispatch::Kind::{Flag, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// The font sources a compile may reach.
///
/// Without this a build could only see the machine's own fonts, which makes the
/// output depend on the machine: a site typeset in a face the author has
/// installed renders in a fallback on CI, out of a green build, and there is no
/// way to ship the face with the site. `paths` is what makes a build
/// reproducible, and `system #false` is what proves it.
#[derive(Debug, Clone, Hash)]
pub struct FontConfig {
    /// Directories scanned recursively for font files, relative to the project
    /// root. Searched before the system's own, so a face a site ships wins over
    /// a same-named one that happens to be installed.
    pub paths: Vec<PathBuf>,
    /// Also use the fonts installed on the machine.
    ///
    /// On by default, because a site that names no directory has nowhere else to
    /// look. Turning it off is what makes a build depend on nothing outside the
    /// project: typst's own bundled faces, plus whatever `paths` names.
    pub system: bool,
}

impl FontConfig {
    /// The first declared directory that is not one, resolved against `root`.
    ///
    /// A scan of a path that is not there yields no faces and says nothing, so a
    /// typo here is a site that silently typesets in a fallback. The caller
    /// turns this into an error before anything compiles, which is the only
    /// moment it can still be told apart from a directory that holds no fonts.
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
            |c, n, t| {
                c.paths = n.words(t)?.into_iter().map(PathBuf::from).collect();
                Ok(())
            },
        ),
        (
            "system",
            Flag,
            "Also use the fonts installed on the machine. Off, a build sees only what the project ships.",
            |c, n, t| {
                c.system = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
