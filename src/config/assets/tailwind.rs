//! `assets { tailwind { } }`: the generated utility stylesheet.

use std::path::PathBuf;

use crate::config::dispatch::Kind::{Asset, Flag, Path as PathKind, Texts};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// A Tailwind-compatible utility stylesheet, generated from the class names the
/// site was written with. Enabled by the presence of an
/// `assets { tailwind { .. } }` block.
///
/// The sheet is an asset the build owns, like any other: it is named
/// `tailwind.css` under the asset tree, fingerprinted, and served only if a page
/// asked for it. A site or theme shipping its own `assets/tailwind.css` keeps
/// that file, which is the override rule every owned asset follows.
#[derive(Debug, Clone, Hash)]
pub struct TailwindConfig {
    /// Whether to generate the sheet.
    pub enabled: bool,
    /// The path the sheet is served from, relative to the asset root.
    ///
    /// The name is the site's, not this crate's: it is written under
    /// `paths { assets }`, linked from every page, and replaced whole by a site
    /// or theme shipping its own file at the same path.
    pub path: PathBuf,
    /// What is read to find class names. A directory is read whole; a file is
    /// read on its own. Empty means the default: the content and template
    /// trees, restricted to the two languages a page can be written in.
    ///
    /// Named rather than globbed, because a glob is a second grammar to explain
    /// and every case this has is a tree: the pages, the templates, and
    /// whatever else a site keeps class names in.
    pub scan: Vec<PathBuf>,
    /// An encre-css configuration file (TOML): the theme, safelist, shortcuts
    /// and preflight. Relative to the project root. Unset, the generator's own
    /// defaults are used, which are Tailwind's.
    pub config: Option<PathBuf>,
    /// Whether the sheet opens with a preflight (the reset rules Tailwind puts
    /// in front of its utilities).
    ///
    /// Only ever read to turn one *off*: a `config` file that states its own
    /// preflight is what decides otherwise, and re-stating that decision in two
    /// places is how they come to disagree.
    pub preflight: bool,
}

impl TailwindConfig {
    /// Whether the sheet is actually generated: configured *and* compiled in.
    /// The shape [`CardsConfig::active`](crate::config::CardsConfig::active)
    /// states, for the same reason: a binary without the generator would
    /// otherwise name a stylesheet in every page's `<head>` that nothing ever
    /// writes.
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

/// The `tailwind { scan ..; config ..; preflight .. }` block. Its presence
/// enables the generated stylesheet.
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
