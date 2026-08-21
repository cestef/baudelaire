//! `content { markdown { } }`: what a `.md` page may contain.

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// What a markdown page's raw HTML does; refused by default, since the DOM a
/// build produces is typed and a string of markup has nowhere to go in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RawHtml {
    #[default]
    Refuse,
    Drop,
}

impl Named for RawHtml {
    const NAMES: &'static [(&'static str, Self)] =
        &[("refuse", Self::Refuse), ("drop", Self::Drop)];
}

/// A markdown parser extension, by the name a site enables it under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Extension {
    /// GFM pipe tables.
    Tables,
    /// GFM footnotes: `[^a]` and its definition.
    Footnotes,
    /// GFM `~~strikethrough~~`.
    Strikethrough,
    /// GFM `- [x]` task lists.
    Tasklists,
    /// Typographic quotes, dashes and ellipses.
    Smart,
    // Math, superscript/subscript and definition lists are absent on purpose:
    // `pulldown-cmark` parses them, but the lowering has no Typst mapping for
    // their events, so enabling one flattens their structure silently.
}

impl Named for Extension {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("tables", Self::Tables),
        ("footnotes", Self::Footnotes),
        ("strikethrough", Self::Strikethrough),
        ("tasklists", Self::Tasklists),
        ("smart", Self::Smart),
    ];
}

impl Extension {
    /// The set a site gets without saying anything: CommonMark plus GFM.
    pub const DEFAULT: &'static [Self] = &[
        Self::Tables,
        Self::Footnotes,
        Self::Strikethrough,
        Self::Tasklists,
    ];

    /// [`DEFAULT`](Self::DEFAULT) by name, for the reference to mark which of
    /// the accepted spellings a site already has.
    pub fn defaults() -> Vec<&'static str> {
        Self::DEFAULT.iter().map(|value| value.name()).collect()
    }
}

/// What a markdown page may contain, which is not whether `.md` is a page at
/// all: that is the `markdown` cargo feature.
#[derive(Debug, Clone, Hash, Table)]
#[table(items {
    /// Writing the block is how a site says it has `.md` pages at all, so a
    /// bare `content { markdown }` is legal.
    fn enable(&mut self, _on: bool) -> bool {
        self.present = true;
        true
    }
})]
pub struct MarkdownConfig {
    /// Whether a `.md` file under `content/` is a page at all.
    #[key(flag)]
    pub enabled: bool,

    /// Whether the site wrote a `markdown` node at all. Only the feature gate
    /// reads it, to warn that a block asking for markdown is doing nothing.
    pub present: bool,

    /// Parser extensions to enable, or `-name` to disable one. A `*` marks the ones already on.
    #[key(toggled(Extension, Extension::DEFAULT, Extension::defaults))]
    pub extensions: Vec<Extension>,

    /// What raw HTML in a page does: `refuse` the build, or `drop` it.
    #[key(choice(RawHtml))]
    pub html: RawHtml,

    /// Whether a fence marked `eval` runs as Typst. Turn it off for content you did not write: it runs at build time.
    #[key(flag)]
    pub eval: bool,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            present: false,
            extensions: Extension::DEFAULT.to_vec(),
            html: RawHtml::default(),
            eval: true,
        }
    }
}
