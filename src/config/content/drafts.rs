//! `content { drafts { } }`: whether drafts build, and what marks one.

use crate::config::dispatch::Kind::{Flag, Text};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// Draft handling: whether drafts build, and the file-stem suffix marking one.
#[derive(Debug, Clone, Hash)]
pub struct DraftConfig {
    /// Build draft pages. Runtime flag, set by `--drafts` or a profile.
    pub build: bool,
    /// Suffix marking draft sources, e.g. `post.draft.typ`.
    pub suffix: String,
}

impl Default for DraftConfig {
    fn default() -> Self {
        Self {
            build: false,
            suffix: ".draft".into(),
        }
    }
}

impl Section for DraftConfig {
    const RULES: Block<Self> = Block(&[
        ("build", Flag, "Build draft pages at all.", |c, n, t| {
            c.build = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "suffix",
            Text,
            "The filename marker that flags a draft, peeled off the stem: `post.draft.typ`.",
            |c, n, t| {
                c.suffix = n.string(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
