//! `assets { tailwind { } }`: the generated utility stylesheet.

use std::path::PathBuf;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// A Tailwind-compatible utility stylesheet, generated from the class names the
/// site was written with. Enabled by the presence of an
/// `assets { tailwind { .. } }` block.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct TailwindConfig {
    pub enabled: bool,

    /// Where the sheet is served from, relative to the asset root.
    #[key(asset)]
    pub path: PathBuf,

    /// The trees and files read to find class names. Unset, the content and template trees.
    ///
    /// A directory is read whole.
    #[key(paths)]
    pub scan: Vec<PathBuf>,

    /// An encre-css configuration file (TOML): theme, safelist, shortcuts, preflight.
    ///
    /// Relative to the project root. Unset, the generator's own defaults are
    /// used.
    #[key(opt path)]
    pub config: Option<PathBuf>,

    /// Whether the sheet opens with the reset rules Tailwind puts in front of its utilities.
    #[key(flag)]
    pub preflight: bool,
}

impl TailwindConfig {
    /// Whether the sheet is actually generated: configured *and* compiled in.
    pub fn active(&self) -> bool {
        self.enabled && cfg!(feature = "tailwind")
    }
}

impl Default for TailwindConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: PathBuf::from("tailwind.css"),
            scan: Vec::new(),
            config: None,
            preflight: true,
        }
    }
}
