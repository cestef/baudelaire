//! `html { highlight { } }`: syntax highlighting as CSS classes.

use crate::config::Named;
use crate::config::dispatch::Kind::{Flag, Table, Text, Toggled};
use crate::config::dispatch::{Block, Keys, Section, Switch};
use crate::config::node::NodeExt;
use crate::error::Result;

/// One class a piece of highlighted code can carry.
///
/// A closed vocabulary, on purpose: a grammar's scopes are its own business
/// (`keyword.other.fn.rust`, `keyword.control.import.python`), and a stylesheet
/// that had to name them would be written once per language. Every grammar
/// funnels into these through the scope table in [`crate::world::rules`], and so
/// does typst's own parser, so one palette covers every language a site will
/// ever highlight.
///
/// Naming is the site's, not this list's: what a token is *called* on the page
/// is [`HighlightConfig::class`], which the vocabulary only supplies a default
/// for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Token {
    Comment,
    String,
    Escape,
    Number,
    Constant,
    Keyword,
    Operator,
    Punctuation,
    Function,
    Type,
    Namespace,
    Tag,
    Attribute,
    Property,
    Variable,
    Parameter,
    Heading,
    Strong,
    Emph,
    Link,
    Raw,
    Label,
    Invalid,
}

impl Named for Token {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("comment", Self::Comment),
        ("string", Self::String),
        ("escape", Self::Escape),
        ("number", Self::Number),
        ("constant", Self::Constant),
        ("keyword", Self::Keyword),
        ("operator", Self::Operator),
        ("punctuation", Self::Punctuation),
        ("function", Self::Function),
        ("type", Self::Type),
        ("namespace", Self::Namespace),
        ("tag", Self::Tag),
        ("attribute", Self::Attribute),
        ("property", Self::Property),
        ("variable", Self::Variable),
        ("parameter", Self::Parameter),
        ("heading", Self::Heading),
        ("strong", Self::Strong),
        ("emph", Self::Emph),
        ("link", Self::Link),
        ("raw", Self::Raw),
        ("label", Self::Label),
        ("invalid", Self::Invalid),
    ];
}

impl Token {
    /// Every token, which is what a site that narrows nothing gets. Read out of
    /// [`NAMES`](Named::NAMES) rather than listed again, so a token cannot be
    /// added to the vocabulary and left out of the default set.
    pub fn all() -> Vec<Self> {
        Self::NAMES.iter().map(|(_, token)| *token).collect()
    }
}

/// Class a code block's tokens instead of colouring them inline, so a stylesheet
/// owns the palette and can follow a light/dark toggle.
///
/// typst-html bakes a highlight theme's colours into a `style="color: .."` on
/// every span, with no option to emit classes. A fixed colour cannot follow a
/// runtime theme switch, so the documented workaround was to author a `.tmTheme`
/// of *meaningless sentinel hex values* and remap each one with
/// `pre code [style*="e5d004"] { color: var(--kw) !important }`: an
/// attribute-substring selector fighting an inline style. That is what this
/// replaces.
///
/// The block's presence is the switch, and `highlight #false` takes it back off
/// again: off, typst's own inline colours are what the page gets, which is the
/// escape hatch for a site that wants the theme it authored rendered as written.
#[derive(Debug, Clone, Hash)]
pub struct HighlightConfig {
    /// Whether to class rather than colour.
    pub enabled: bool,
    /// What every class starts with. Empty emits the bare token name.
    pub prefix: String,
    /// The tokens that reach the page. One a site does not style is markup it
    /// pays for and never uses, so narrowing this is how a minimal palette gets
    /// minimal markup.
    pub tokens: Vec<Token>,
    /// Per-token renames, prefix aside: `keyword "kw"` writes `sx-kw`. What a
    /// site with an existing stylesheet needs, since a class name it already
    /// ships is not one this crate gets to choose.
    pub classes: Vec<(Token, String)>,
    /// Also stamp each classed span with the grammar's own scope, as
    /// `data-scope`. The escape hatch out of the vocabulary: a stylesheet that
    /// wants to tell one kind of keyword from another can select on
    /// `[data-scope^="keyword.control"]` without this crate inventing a token
    /// for it.
    pub scopes: bool,
}

impl Default for HighlightConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prefix: Self::PREFIX.to_owned(),
            tokens: Token::all(),
            classes: Vec::new(),
            scopes: false,
        }
    }
}

impl HighlightConfig {
    /// The class prefix a site gets without asking: short, and unlikely to
    /// collide with anything a stylesheet already has.
    const PREFIX: &'static str = "sx-";

    /// The class `token` is written as, or `None` when the site has dropped it
    /// and the span should not be emitted at all.
    ///
    /// The single naming rule: the show rule marks a piece with the token it
    /// resolved to, and this is the only thing that turns one into markup.
    pub fn class(&self, token: Token) -> Option<String> {
        if !self.tokens.contains(&token) {
            return None;
        }
        let name = self
            .classes
            .iter()
            .find(|(named, _)| *named == token)
            .map_or_else(|| token.name(), |(_, class)| class.as_str());
        Some(format!("{}{name}", self.prefix))
    }
}

impl Section for HighlightConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "prefix",
            Text,
            "What every emitted class starts with. Empty for none.",
            |c, n, t| {
                c.prefix = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "tokens",
            Toggled(Token::names, Token::names),
            "The tokens that reach the page, or `-name` to drop one. All are on.",
            |c, n, t| {
                c.tokens = n.toggled::<Token>(t, &Token::all())?;
                Ok(())
            },
        ),
        (
            "classes",
            Table,
            "Per-token class names, prefix aside: `keyword \"kw\"` writes `sx-kw`.",
            |c, n, t| {
                // Read node by node rather than through `NodeExt::pairs`, which
                // hands back strings: a token that is not one is reported at the
                // word the author wrote, not at the block holding it.
                c.classes = n
                    .block(t)?
                    .nodes()
                    .iter()
                    .map(|entry| {
                        let name = entry.name().value();
                        let token = Token::of(name).ok_or_else(|| {
                            Keys::unknown_value(Token::NAMES, t, name, NodeExt::span(entry))
                        })?;
                        Ok((token, entry.string(t, 0)?))
                    })
                    .collect::<Result<_>>()?;
                Ok(())
            },
        ),
        (
            "scopes",
            Flag,
            "Also stamp each span with the grammar's own scope, as `data-scope`.",
            |c, n, t| {
                c.scopes = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
