//! Injects browser-native navigation hints into each page's `<head>`.
//!
//! A `<script type="speculationrules">` asks the browser to fetch, or fully
//! render, an internal link's target before it is clicked. It is the
//! zero-JavaScript neighbour of the SPA runtime: no code ships, nothing mounts,
//! and a browser without the API ignores the script entirely.

use serde::Serialize;
use typst::syntax::Span;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{Config, Eagerness, Named};

use super::{Cx, ElementExt, Transform};

/// The [`Transform`] that appends speculation rules to `<head>`.
pub(super) struct Speculation;

impl Transform for Speculation {
    fn enabled(&self, config: &Config) -> bool {
        config.speculation.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let Some(rules) = Rules::of(cx.config) else {
            return;
        };
        // Best-effort, like every other transform: a page that emitted its own
        // root has no `<head>` to append to, and gains no hints.
        if let Some(head) = doc.root_mut().head() {
            head.children.push(rules.script());
        }
    }
}

/// The rule document, one list per action. Absent actions are omitted rather
/// than emitted empty, which the API would reject.
#[derive(Serialize)]
struct Rules {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    prefetch: Vec<Rule>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    prerender: Vec<Rule>,
}

/// One rule: which links it covers, and how eagerly to act on them.
#[derive(Serialize)]
struct Rule {
    /// A URL-pattern match. Every page of this site, and only this site: a
    /// subpath-hosted site matches under its own prefix so the rules never
    /// speculate on a neighbour sharing the host.
    #[serde(rename = "where")]
    scope: Scope,
    eagerness: &'static str,
}

#[derive(Serialize)]
struct Scope {
    href_matches: String,
}

impl Rules {
    /// The rules for a config, or `None` when both actions are off (a
    /// `speculation { }` block that turned everything down would otherwise
    /// emit an empty, meaningless script).
    fn of(config: &Config) -> Option<Self> {
        let scope = config.prefixed("/*");
        let rule = |eagerness: Eagerness| match eagerness {
            Eagerness::None => Vec::new(),
            set => vec![Rule {
                scope: Scope {
                    href_matches: scope.clone(),
                },
                eagerness: set.name(),
            }],
        };
        let rules = Self {
            prefetch: rule(config.speculation.prefetch),
            prerender: rule(config.speculation.prerender),
        };
        (!rules.prefetch.is_empty() || !rules.prerender.is_empty()).then_some(rules)
    }

    /// The rule document as the `<script>` element that carries it.
    fn script(&self) -> HtmlNode {
        let mut el = HtmlElement::new(tag::script);
        el.attrs.push(attr::r#type, "speculationrules");
        // Infallible: every field is a plain string or list of them, and
        // `serde_json` only fails on a serializer that can (a map with
        // non-string keys, a non-finite float, a custom error).
        let json = serde_json::to_string(self).expect("rules are plain strings");
        el.children
            .push(HtmlNode::Text(json.into(), Span::detached()));
        HtmlNode::Element(el)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(prefetch: Eagerness, prerender: Eagerness) -> Config {
        let mut config = Config::default();
        config.speculation.enabled = true;
        config.speculation.prefetch = prefetch;
        config.speculation.prerender = prerender;
        config
    }

    fn json(config: &Config) -> serde_json::Value {
        let rules = Rules::of(config).expect("rules");
        serde_json::from_str(&serde_json::to_string(&rules).unwrap()).unwrap()
    }

    #[test]
    fn names_the_configured_eagerness_per_action() {
        let value = json(&config(Eagerness::Moderate, Eagerness::Conservative));
        assert_eq!(value["prefetch"][0]["eagerness"], "moderate");
        assert_eq!(value["prerender"][0]["eagerness"], "conservative");
        assert_eq!(value["prefetch"][0]["where"]["href_matches"], "/*");
    }

    /// An action set to `none` is left out entirely: an empty list is not a
    /// valid rule set.
    #[test]
    fn omits_an_action_that_is_off() {
        let value = json(&config(Eagerness::Eager, Eagerness::None));
        assert!(value.get("prerender").is_none(), "{value}");
        assert_eq!(value["prefetch"][0]["eagerness"], "eager");
    }

    /// Both off means no script at all, rather than an empty one.
    #[test]
    fn produces_nothing_when_both_actions_are_off() {
        assert!(Rules::of(&config(Eagerness::None, Eagerness::None)).is_none());
    }

    /// A subpath-hosted site speculates only under its own prefix, never on a
    /// neighbouring site that happens to share the host.
    #[test]
    fn scopes_the_pattern_to_the_base_path() {
        let mut config = config(Eagerness::Moderate, Eagerness::None);
        config.url = Some("https://host.test/docs".into());
        let value = json(&config);
        assert_eq!(value["prefetch"][0]["where"]["href_matches"], "/docs/*");
    }
}
