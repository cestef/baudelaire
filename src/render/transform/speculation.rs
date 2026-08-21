//! Injects a `<script type="speculationrules">` into each page's `<head>`,
//! asking the browser to fetch or render an internal link before it is clicked.

use serde::Serialize;
use typst::syntax::Span;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{Config, Eagerness, Named};

use super::{Cx, DocumentExt, Exempt, Transform};

/// The [`Transform`] that appends speculation rules to `<head>`.
pub(super) struct Speculation;

impl Speculation {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "speculation";
}

impl Transform for Speculation {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

    fn enabled(&self, config: &Config) -> bool {
        config.navigation.speculation.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let Some(rules) = Rules::of(cx.config) else {
            return;
        };
        if let Some(head) = doc.head() {
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
    /// A URL-pattern match covering this site only, so a subpath-hosted site
    /// never speculates on a neighbour sharing the host.
    #[serde(rename = "where")]
    scope: Scope,
    eagerness: &'static str,
}

#[derive(Serialize)]
struct Scope {
    href_matches: String,
}

impl Rules {
    /// The rules for a config, or `None` when both actions are off.
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
            prefetch: rule(config.navigation.speculation.prefetch),
            prerender: rule(config.navigation.speculation.prerender),
        };
        (!rules.prefetch.is_empty() || !rules.prerender.is_empty()).then_some(rules)
    }

    /// The rule document as the `<script>` element that carries it.
    fn script(&self) -> HtmlNode {
        let json = serde_json::to_string(self).expect("rules are plain strings");
        let mut el = HtmlElement::new(tag::script).with_attr(attr::r#type, "speculationrules");
        el.children
            .push(HtmlNode::Text(json.into(), Span::detached()));
        el.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(prefetch: Eagerness, prerender: Eagerness) -> Config {
        let mut config = Config::default();
        config.navigation.speculation.enabled = true;
        config.navigation.speculation.prefetch = prefetch;
        config.navigation.speculation.prerender = prerender;
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

    #[test]
    fn omits_an_action_that_is_off() {
        let value = json(&config(Eagerness::Eager, Eagerness::None));
        assert!(value.get("prerender").is_none(), "{value}");
        assert_eq!(value["prefetch"][0]["eagerness"], "eager");
    }

    #[test]
    fn produces_nothing_when_both_actions_are_off() {
        assert!(Rules::of(&config(Eagerness::None, Eagerness::None)).is_none());
    }

    #[test]
    fn scopes_the_pattern_to_the_base_path() {
        let mut config = config(Eagerness::Moderate, Eagerness::None);
        config.url = Some("https://host.test/docs".into());
        let value = json(&config);
        assert_eq!(value["prefetch"][0]["where"]["href_matches"], "/docs/*");
    }
}
