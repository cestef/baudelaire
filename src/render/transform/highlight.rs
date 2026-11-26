//! Names the highlight marks the `raw` show rule left behind, applying
//! `html { highlight { } }`. No mark survives this pass.

use typst::ecow::EcoVec;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{Config, HighlightConfig, Named, Token};
use crate::world::rules::{SCOPE, TOKEN};

use super::{AttrsExt, Cx, DocumentExt, Exempt, Transform};

/// The [`Transform`] that names highlight marks.
pub(super) struct Highlight;

impl Transform for Highlight {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

    fn enabled(&self, config: &Config) -> bool {
        config.html.highlight.enabled
    }

    /// Keyed on `<code>` rather than the `<pre>` wrapper, which only a block
    /// raw has.
    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let highlight = &cx.config.html.highlight;
        doc.walk(|element| {
            if element.tag == tag::code {
                Self::name(element, highlight);
            }
        });
    }
}

impl Highlight {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "highlight";

    /// Name every marked span under `element`, depth first: a token becomes
    /// its class, a scope stays only when `scopes` asks for it, and a span left
    /// with neither is spliced back into plain text.
    fn name(element: &mut HtmlElement, config: &HighlightConfig) {
        let mut dropped = false;
        for node in element.children.make_mut() {
            let HtmlNode::Element(child) = node else {
                continue;
            };
            Self::name(child, config);
            if !Self::marked(child) {
                continue;
            }
            let class = Self::token(child).and_then(|token| config.class(token));
            child.attrs.remove(TOKEN);
            match (config.scopes, Self::scope(child)) {
                (true, Some(scope)) => child.attrs.set(SCOPE, &scope),
                _ => child.attrs.remove(SCOPE),
            }
            match class {
                Some(class) => child.attrs.set(attr::class, &class),
                None => dropped |= child.attrs.0.is_empty(),
            }
        }
        if dropped {
            Self::splice(element);
        }
    }

    /// Whether `element` is a span the show rule marked.
    fn marked(element: &HtmlElement) -> bool {
        element.tag == tag::span
            && (element.attrs.get(TOKEN).is_some() || element.attrs.get(SCOPE).is_some())
    }

    /// The token a marked span names, if it names one.
    fn token(element: &HtmlElement) -> Option<Token> {
        Token::of(element.attrs.get(TOKEN)?)
    }

    /// The `data-scope` stamp a marked span keeps: its innermost scope.
    fn scope(element: &HtmlElement) -> Option<String> {
        let stack = element.attrs.get(SCOPE)?;
        stack.rsplit(' ').next().map(str::to_owned)
    }

    /// Replace each span this pass emptied with the text it wraps, keeping
    /// every other node where it is. Emptied means no attribute left: a marked
    /// span carries only the two marks, so nothing else can be bare.
    fn splice(element: &mut HtmlElement) {
        let mut children = EcoVec::with_capacity(element.children.len());
        for node in &element.children {
            match node {
                HtmlNode::Element(child) if child.tag == tag::span && child.attrs.0.is_empty() => {
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
    fn block(pieces: &[(Option<Token>, &str, &str)]) -> HtmlElement {
        let mut pre = HtmlElement::new(tag::code);
        for (token, scope, text) in pieces {
            let mut span = HtmlElement::new(tag::span);
            if let Some(token) = token {
                span.attrs.push(TOKEN, token.name());
            }
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
        pieces: &[(Option<Token>, &str, &str)],
    ) -> Vec<(Option<String>, Option<String>, String)> {
        let mut pre = block(pieces);
        Highlight::name(&mut pre, config);
        named(&pre)
    }

    #[test]
    fn a_mark_becomes_a_prefixed_class_and_loses_its_scope() {
        let out = rewritten(
            &HighlightConfig::default(),
            &[(Some(Token::Keyword), "keyword.control.rust", "let")],
        );
        assert_eq!(
            out,
            vec![(Some("sx-keyword".to_owned()), None, "let".to_owned())]
        );
    }

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
                (Some(Token::Keyword), "keyword.control.rust", "let"),
                (Some(Token::String), "string.quoted.rust", "\"a\""),
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

    #[test]
    fn a_dropped_token_leaves_its_text_behind() {
        let config = HighlightConfig {
            tokens: vec![Token::Keyword],
            ..HighlightConfig::default()
        };
        let out = rewritten(
            &config,
            &[
                (Some(Token::Keyword), "keyword.control.rust", "let"),
                (Some(Token::Punctuation), "punctuation.separator.rust", ";"),
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

    #[test]
    fn a_scope_with_no_token_keeps_a_span_only_when_scopes_are_asked_for() {
        let piece = [(None, "source.v entity.name.constant.v", ".generate")];
        assert_eq!(
            rewritten(&HighlightConfig::default(), &piece),
            vec![(None, None, ".generate".to_owned())]
        );
        let config = HighlightConfig {
            scopes: true,
            ..HighlightConfig::default()
        };
        assert_eq!(
            rewritten(&config, &piece),
            vec![(
                None,
                Some("entity.name.constant.v".to_owned()),
                ".generate".to_owned()
            )]
        );
    }

    #[test]
    fn the_scope_stamp_survives_when_it_is_asked_for() {
        let config = HighlightConfig {
            scopes: true,
            ..HighlightConfig::default()
        };
        let out = rewritten(&config, &[(Some(Token::Keyword), "keyword.control.rust", "let")]);
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
