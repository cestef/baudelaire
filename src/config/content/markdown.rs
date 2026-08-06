//! `content { markdown { } }`: what a `.md` page may contain.

use crate::config::Named;
use crate::config::dispatch::Kind::{Choice, Flag, Toggled};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// What a markdown page's raw HTML does.
///
/// Refusing is the default because the DOM a build produces is typed: a string
/// of markup has nowhere to be spliced into it, and silently dropping it
/// publishes a page missing something the author wrote. A site importing
/// content it did not write may prefer the quieter answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RawHtml {
    /// Fail the build, naming the file and underlining the markup.
    #[default]
    Refuse,
    /// Leave it out and carry on.
    Drop,
}

impl Named for RawHtml {
    const NAMES: &'static [(&'static str, Self)] =
        &[("refuse", Self::Refuse), ("drop", Self::Drop)];
}

/// A markdown parser extension, by the name a site enables it under.
///
/// The set is closed and every variant is listed once, in [`Named::NAMES`], so
/// the parser, the config reference and the error that suggests a near miss all
/// read one table.
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
    // Math, superscript/subscript and definition lists are deliberately absent.
    // `pulldown-cmark` parses all three, but the lowering has no Typst mapping
    // for their events yet, so enabling one produced text with its structure
    // silently flattened out -- concatenated definition terms, `$x^2$` set as
    // prose rather than as an equation. A key that parses and then produces
    // wrong output is worse than one that does not exist, so they arrive with
    // their `Tag` arms or not at all.
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
    /// The set a site gets without saying anything: CommonMark plus GFM, which
    /// is what "markdown" means to most people writing it.
    pub const DEFAULT: &'static [Self] = &[
        Self::Tables,
        Self::Footnotes,
        Self::Strikethrough,
        Self::Tasklists,
    ];

    /// [`DEFAULT`](Self::DEFAULT) by name, for the reference to mark which of
    /// the accepted spellings a site already has. Derived, so the documented
    /// default set cannot drift from the one that is applied.
    pub fn defaults() -> Vec<&'static str> {
        Self::DEFAULT.iter().map(|value| value.name()).collect()
    }
}

/// How markdown pages are read.
///
/// Every field is a capability of the *site*, not of the binary: the `markdown`
/// cargo feature decides whether `.md` is a page at all, and this decides what
/// one may contain once it is.
#[derive(Debug, Clone, Hash)]
pub struct MarkdownConfig {
    /// Whether a `.md` file under `content/` is a page.
    ///
    /// On when the binary has the feature, so a site says nothing to get
    /// markdown pages. Turning it *off* is the useful direction: a site with
    /// `.md` files it does not publish -- a README, notes beside the pages --
    /// says `markdown #false` and they go back to being ignored.
    pub enabled: bool,
    /// Whether the site wrote a `markdown` node at all.
    ///
    /// Distinct from `enabled`, and only the feature gate reads it: it is how a
    /// binary built *without* the feature can say that a block asking for
    /// something is doing nothing, rather than ignoring it in silence. A site
    /// that never mentions markdown is not warned about a capability it never
    /// asked for.
    pub present: bool,
    /// The parser extensions in force.
    pub extensions: Vec<Extension>,
    /// What raw HTML in a page does.
    pub html: RawHtml,
    /// Whether a fence marked `eval` runs as Typst.
    ///
    /// On by default, and worth turning off deliberately: an `eval` fence runs
    /// arbitrary Typst at build time, so a site building contributed or
    /// imported markdown should refuse it rather than trust every author.
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

/// What a markdown page may contain. Nothing here decides whether `.md` is a
/// page at all: that is the `markdown` cargo feature. A binary built without it
/// still parses this block and then ignores it, since no `.md` file is a page
/// for it to apply to.
impl Section for MarkdownConfig {
    const RULES: Block<Self> = Block(&[
        (
            "enabled",
            Flag,
            "Whether a `.md` file under `content/` is a page at all.",
            |c, n, t| {
                c.enabled = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "extensions",
            Toggled(Extension::names, Extension::defaults),
            "Parser extensions to enable, or `-name` to disable one. A `*` marks the ones already on.",
            |c, n, t| {
                c.extensions = n.toggled::<Extension>(t, Extension::DEFAULT)?;
                Ok(())
            },
        ),
        (
            "html",
            Choice(RawHtml::names),
            "What raw HTML in a page does: `refuse` the build, or `drop` it.",
            |c, n, t| {
                c.html = n.arg(t, 0)?.one::<RawHtml>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "eval",
            Flag,
            "Whether a fence marked `eval` runs as Typst. Turn it off for content you did not write: it runs at build time.",
            |c, n, t| {
                c.eval = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);

    /// Writing the block is how a site says it needs markdown at all, which is
    /// what lets a binary built without the feature answer. Returning `true`
    /// makes a bare `content { markdown }` legal, and that is its whole meaning:
    /// "this site has `.md` pages".
    ///
    /// Not a [`Section::SWITCH`], even though `markdown #false` reads like one:
    /// this section is dispatched through [`Section::shorthand`], which hands
    /// the line's boolean to the `enabled` key's own handler, so the one
    /// documented key and the short spelling are one code path. `on` is
    /// therefore always `true` here and says nothing.
    fn enable(&mut self, _on: bool) -> bool {
        self.present = true;
        true
    }
}
