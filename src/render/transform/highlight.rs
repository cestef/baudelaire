//! Names the highlight marks the `raw` show rule left behind.
//!
//! [`crate::world::rules`] marks each highlighted piece with the vocabulary
//! entry its scopes resolved to (`data-token`) and with the grammar's own scope
//! (`data-scope`). A show rule is a bare `fn` with nowhere to keep a config, so
//! it cannot know what the site calls those, and this is where
//! `html { highlight { } }` is applied: the mark becomes a class, keeps or loses
//! its scope stamp, and a token the site dropped loses its span entirely.
//!
//! No mark survives this pass, which is the contract: an unnamed `data-token` in
//! the output would mean the transform did not run over markup the rule emitted.

use typst::ecow::EcoVec;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{Config, HighlightConfig, Named, Token};
use crate::world::rules::{SCOPE, TOKEN};

use super::{AttrsExt, Cx, DocumentExt, Transform};

/// The [`Transform`] that names highlight marks.
pub(super) struct Highlight;

impl Transform for Highlight {
    fn enabled(&self, config: &Config) -> bool {
        config.html.highlight.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let highlight = &cx.config.html.highlight;
        doc.walk(|element| {
            // Inside a `<code>`, which is what both spellings of a code block
            // have: the block one is wrapped in a `<pre>` and the inline one is
            // not, and keying on the wrapper left every `#raw("..", lang: ..)`
            // in a sentence marked and unnamed.
            if element.tag == tag::code {
                Self::name(element, highlight);
            }
        });
    }
}

impl Highlight {
    /// Name every marked span under `element`, depth first.
    fn name(element: &mut HtmlElement, config: &HighlightConfig) {
        let mut dropped = false;
        for node in element.children.make_mut() {
            let HtmlNode::Element(child) = node else {
                continue;
            };
            Self::name(child, config);
            let Some(token) = Self::marked(child) else {
                continue;
            };
            match config.class(token) {
                Some(class) => {
                    child.attrs.set(attr::class, &class);
                    child.attrs.remove(TOKEN);
                    if !config.scopes {
                        child.attrs.remove(SCOPE);
                    }
                }
                // Left marked, so the splice below can find it again: a token
                // the site turned off is a span it never asked to pay for.
                None => dropped = true,
            }
        }
        if dropped {
            Self::splice(element);
        }
    }

    /// The token a span is marked with, or `None` when it carries no mark of
    /// ours.
    fn marked(element: &HtmlElement) -> Option<Token> {
        if element.tag != tag::span {
            return None;
        }
        Token::of(element.attrs.get(TOKEN)?)
    }

    /// Replace each still-marked child with the text it wraps, keeping every
    /// other node where it is.
    fn splice(element: &mut HtmlElement) {
        let mut children = EcoVec::with_capacity(element.children.len());
        for node in &element.children {
            match node {
                HtmlNode::Element(child) if Self::marked(child).is_some() => {
                    children.extend(child.children.iter().cloned());
                }
                other => children.push(other.clone()),
            }
        }
        element.children = children;
    }
}

#[cfg(test)]
mod tests {
    use super::{Highlight, SCOPE, TOKEN};
    use crate::config::{HighlightConfig, Named, Token};
    use typst::syntax::Span;
    use typst_html::{HtmlElement, HtmlNode, attr, tag};

    /// A `<code>` holding one marked span per `(token, scope, text)`, as the
    /// show rule emits them.
    fn block(pieces: &[(Token, &str, &str)]) -> HtmlElement {
        let mut pre = HtmlElement::new(tag::code);
        for (token, scope, text) in pieces {
            let mut span = HtmlElement::new(tag::span);
            span.attrs.push(TOKEN, token.name());
            span.attrs.push(SCOPE, *scope);
            span.children
                .push(HtmlNode::Text((*text).into(), Span::detached()));
            pre.children.push(span.into());
        }
        pre
    }

    /// Every span, as `(class, scope, text)`; a spliced one is a bare text node
    /// and reads as `None`.
    fn named(pre: &HtmlElement) -> Vec<(Option<String>, Option<String>, String)> {
        pre.children
            .iter()
            .map(|node| match node {
                HtmlNode::Element(el) => (
                    el.attrs.get(attr::class).map(ToString::to_string),
                    el.attrs.get(SCOPE).map(ToString::to_string),
                    el.children
                        .iter()
                        .map(|child| match child {
                            HtmlNode::Text(text, _) => text.to_string(),
                            _ => String::new(),
                        })
                        .collect(),
                ),
                HtmlNode::Text(text, _) => (None, None, text.to_string()),
                _ => (None, None, String::new()),
            })
            .collect()
    }

    fn rewritten(
        config: &HighlightConfig,
        pieces: &[(Token, &str, &str)],
    ) -> Vec<(Option<String>, Option<String>, String)> {
        let mut pre = block(pieces);
        Highlight::name(&mut pre, config);
        named(&pre)
    }

    #[test]
    fn a_mark_becomes_a_prefixed_class_and_loses_its_scope() {
        let out = rewritten(
            &HighlightConfig::default(),
            &[(Token::Keyword, "keyword.control.rust", "let")],
        );
        assert_eq!(
            out,
            vec![(Some("sx-keyword".to_owned()), None, "let".to_owned())]
        );
    }

    /// Both halves of the naming: the prefix a site chose, and the name it gave
    /// one token.
    #[test]
    fn a_configured_prefix_and_rename_both_land() {
        let config = HighlightConfig {
            prefix: "tok-".to_owned(),
            classes: vec![(Token::Keyword, "kw".to_owned())],
            ..HighlightConfig::default()
        };
        let out = rewritten(
            &config,
            &[
                (Token::Keyword, "keyword.control.rust", "let"),
                (Token::String, "string.quoted.rust", "\"a\""),
            ],
        );
        assert_eq!(
            out,
            vec![
                (Some("tok-kw".to_owned()), None, "let".to_owned()),
                (Some("tok-string".to_owned()), None, "\"a\"".to_owned()),
            ]
        );
    }

    /// A dropped token keeps its text and loses its span: the point of dropping
    /// one is the markup, so leaving an empty span behind would buy nothing.
    #[test]
    fn a_dropped_token_leaves_its_text_behind() {
        let config = HighlightConfig {
            tokens: vec![Token::Keyword],
            ..HighlightConfig::default()
        };
        let out = rewritten(
            &config,
            &[
                (Token::Keyword, "keyword.control.rust", "let"),
                (Token::Punctuation, "punctuation.separator.rust", ";"),
            ],
        );
        assert_eq!(
            out,
            vec![
                (Some("sx-keyword".to_owned()), None, "let".to_owned()),
                (None, None, ";".to_owned()),
            ]
        );
    }

    /// The escape hatch: the grammar's own scope stays on the page for a
    /// stylesheet that wants to select finer than the vocabulary can.
    #[test]
    fn the_scope_stamp_survives_when_it_is_asked_for() {
        let config = HighlightConfig {
            scopes: true,
            ..HighlightConfig::default()
        };
        let out = rewritten(&config, &[(Token::Keyword, "keyword.control.rust", "let")]);
        assert_eq!(
            out,
            vec![(
                Some("sx-keyword".to_owned()),
                Some("keyword.control.rust".to_owned()),
                "let".to_owned()
            )]
        );
    }
}
