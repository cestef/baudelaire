//! `assets { sourcemap }`: what becomes of the source map for each kind of
//! processed asset.

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::vocab::rule;
use crate::error::{ConfigError, Result};

/// What is done with the source map for one kind of asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SourceMaps {
    #[default]
    Off,
    /// Written into the asset itself, as a `data:` URI on the end of it.
    Inline,
    /// Written beside the asset, named by a `sourceMappingURL` comment.
    External,
    /// Written beside the asset and named by nothing at all; the file is still
    /// served, so this hides the map rather than protecting it.
    Hidden,
}

impl Named for SourceMaps {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("off", Self::Off),
        ("inline", Self::Inline),
        ("external", Self::External),
        ("hidden", Self::Hidden),
    ];
}

impl SourceMaps {
    /// Whether a map is produced at all.
    pub fn wanted(self) -> bool {
        self != Self::Off
    }

    pub fn inline(self) -> bool {
        self == Self::Inline
    }

    /// Whether the asset carries a comment naming its map, which an inline map
    /// is.
    pub fn linked(self) -> bool {
        matches!(self, Self::Inline | Self::External)
    }
}

/// The `assets { sourcemap }` section: one [`SourceMaps`] per kind of asset the
/// pipeline can map.
#[derive(Debug, Clone, Hash, Default, Table)]
#[table(items {
    /// The value on the section's own line, which stands for every kind at
    /// once.
    const LEADING: usize = 1;

    /// `sourcemap "external"` sets every kind; a block narrows it per kind.
    ///
    /// A node with neither a value nor a block is an error: a default posture
    /// invented here would be re-applied by every profile naming the section,
    /// silently undoing the base config.
    fn fill(&mut self, node: &KdlNode, text: &str) -> Result<()> {
        Self::line(node, text)?;
        let stated = node.entries().iter().any(|entry| entry.name().is_none());
        if stated {
            let all = node
                .arg(text, 0)?
                .one::<SourceMaps>(text, NodeExt::span(node))?;
            self.scripts = all;
            self.styles = all;
        }
        match node.children() {
            Some(block) => Self::RULES.apply(self, block.nodes(), text),
            None if stated => Ok(()),
            None => Err(ConfigError::missing_children(text, NodeExt::span(node)).into()),
        }
    }
})]
pub struct SourceMapConfig {
    /// What becomes of the source map for a bundled script.
    #[key(choice(SourceMaps))]
    pub scripts: SourceMaps,

    /// What becomes of the source map for a processed stylesheet.
    #[key(choice(SourceMaps))]
    pub styles: SourceMaps,
}
