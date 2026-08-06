//! `assets { sourcemap }`: what becomes of the source map for each kind of
//! processed asset.

use kdl::KdlNode;

use crate::config::Named;
use crate::config::dispatch::Kind::Choice;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::error::{ConfigError, Result};

/// What is done with the source map for one kind of asset.
///
/// One value rather than a set of flags, because the choices are exclusive and
/// the combinations are not all meaningful: a map cannot be inline *and*
/// unreferenced, and "written but linked from nothing" is a deliberate posture
/// rather than the absence of a link. Spelled as four words, each of which
/// answers the only question a source map really poses, which is who is meant
/// to read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SourceMaps {
    /// None written, and nothing disclosed. The default.
    #[default]
    Off,
    /// Written into the asset itself, as a `data:` URI on the end of it. One
    /// file and no second request, at the cost of shipping the map to every
    /// visitor whether or not anybody opens it.
    Inline,
    /// Written beside the asset, named by a `sourceMappingURL` comment. What a
    /// browser follows when devtools are open, and nothing fetches otherwise.
    External,
    /// Written beside the asset and named by nothing at all.
    ///
    /// For uploading to an error tracker: the map exists for the build that
    /// produced it, and a visitor is never told where to find it. Note the file
    /// is still *served*, so this hides the map rather than protecting it; it is
    /// a posture, not an access control.
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

    /// Whether the map travels inside the asset rather than beside it.
    pub fn inline(self) -> bool {
        self == Self::Inline
    }

    /// Whether the asset carries a comment naming its map.
    ///
    /// True for [`Inline`](Self::Inline) too: an inline map *is* that comment,
    /// carrying the map in place of a filename.
    pub fn linked(self) -> bool {
        matches!(self, Self::Inline | Self::External)
    }
}

/// The `assets { sourcemap }` section: one [`SourceMaps`] per kind of asset the
/// pipeline can map.
#[derive(Debug, Clone, Hash, Default)]
pub struct SourceMapConfig {
    /// Bundled JavaScript, which is to say whatever `assets { bundle }` built.
    pub scripts: SourceMaps,
    /// Stylesheets the pipeline processed.
    pub styles: SourceMaps,
}

impl Section for SourceMapConfig {
    /// The value on the section's own line, which stands for every kind at
    /// once.
    const LEADING: usize = 1;

    const RULES: Block<Self> = Block(&[
        (
            "scripts",
            Choice(SourceMaps::names),
            "What becomes of the source map for a bundled script.",
            |c, n, t| {
                c.scripts = n.arg(t, 0)?.one::<SourceMaps>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "styles",
            Choice(SourceMaps::names),
            "What becomes of the source map for a processed stylesheet.",
            |c, n, t| {
                c.styles = n.arg(t, 0)?.one::<SourceMaps>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
    ]);

    /// `sourcemap "external"` sets every kind; a block then narrows it per kind.
    ///
    /// Written out rather than left to [`Section::shorthand`], which stands for
    /// exactly one key: here the line stands for *all* of them, and the two
    /// spellings have to compose, so `sourcemap "external" { styles "off" }`
    /// reads as the sentence it looks like.
    ///
    /// The value is required, and deliberately. A bare `sourcemap` would have to
    /// invent a default posture, and the same default would then be re-applied
    /// by every profile that names the section, silently undoing whatever the
    /// base config chose. Leaving the line empty instead means "change nothing
    /// but what my block says", which is what fill-in-place promises everywhere
    /// else.
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
            // Neither a value nor a block: the node asks for nothing, and
            // guessing what it meant is how a profile silently overrides its
            // base.
            None => Err(ConfigError::missing_children(text, NodeExt::span(node)).into()),
        }
    }
}
