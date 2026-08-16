//! `html { highlight { } }`: syntax highlighting as CSS classes.

use crate::config::Named;
use crate::config::dispatch::Kind::{Flag, Table, Text, Toggled};
use crate::config::dispatch::{Block, Keys, Section, Switch};
use crate::config::node::NodeExt;
use crate::error::Result;

/// One class a piece of highlighted code can carry.
///
/// A closed vocabulary: every grammar funnels into it through the scope table
/// in [`crate::world::rules`].
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
    /// Every token, which is what a site that narrows nothing gets.
    pub fn all() -> Vec<Self> {
        Self::NAMES.iter().map(|(_, token)| *token).collect()
    }
}

/// Class a code block's tokens instead of colouring them inline, so a
/// stylesheet owns the palette and can follow a light/dark toggle.
///
/// Off, typst's own inline colours are what the page gets.
#[derive(Debug, Clone, Hash)]
pub struct HighlightConfig {
    /// Whether to class rather than colour.
    pub enabled: bool,
    /// What every class starts with. Empty emits the bare token name.
    pub prefix: String,
    /// The tokens that reach the page.
    pub tokens: Vec<Token>,
    /// Per-token renames, prefix aside: `keyword "kw"` writes `sx-kw`.
    pub classes: Vec<(Token, String)>,
    /// Also stamp each classed span with the grammar's own scope, as
    /// `data-scope`.
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
    const PREFIX: &'static str = "sx-";

    /// The class `token` is written as, or `None` when the site has dropped it
    /// and the span should not be emitted at all.
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
