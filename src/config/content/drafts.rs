//! `content { drafts { } }`: whether drafts build, and what marks one.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

#[derive(Debug, Clone, Hash, Table)]
pub struct DraftConfig {
    /// Build draft pages at all.
    ///
    /// Set by `--drafts` or a profile, not only by this block.
    #[key(flag)]
    pub build: bool,

    /// The filename marker that flags a draft, peeled off the stem: `post.draft.typ`.
    #[key(text)]
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
