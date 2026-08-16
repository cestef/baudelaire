//! `assets { tailwind { } }`: the generated utility stylesheet.

use std::path::PathBuf;

use crate::config::dispatch::Kind::{Asset, Flag, Path as PathKind, Texts};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// A Tailwind-compatible utility stylesheet, generated from the class names the
/// site was written with. Enabled by the presence of an
/// `assets { tailwind { .. } }` block.
#[derive(Debug, Clone, Hash)]
pub struct TailwindConfig {
    pub enabled: bool,
    /// Where the sheet is served from, relative to the asset root.
    pub path: PathBuf,
    /// What is read to find class names; a directory is read whole. Empty means
    /// the content and template trees.
    pub scan: Vec<PathBuf>,
    /// An encre-css configuration file (TOML), relative to the project root.
    /// Unset, the generator's own defaults are used.
    pub config: Option<PathBuf>,
    /// Whether the sheet opens with a preflight (the reset rules Tailwind puts
    /// in front of its utilities).
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

impl Section for TailwindConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "path",
            Asset,
            "Where the sheet is served from, relative to the asset root.",
            |c, n, t| {
                c.path = n.asset(t, 0)?;
                Ok(())
            },
        ),
        (
            "scan",
            Texts,
            "The trees and files read to find class names. Unset, the content and template trees.",
            |c, n, t| {
                c.scan = n.words(t)?.into_iter().map(PathBuf::from).collect();
                Ok(())
            },
        ),
        (
            "config",
            PathKind,
            "An encre-css configuration file (TOML): theme, safelist, shortcuts, preflight.",
            |c, n, t| {
                c.config = Some(n.string(t, 0)?.into());
                Ok(())
            },
        ),
        (
            "preflight",
            Flag,
            "Whether the sheet opens with the reset rules Tailwind puts in front of its utilities.",
            |c, n, t| {
                c.preflight = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
