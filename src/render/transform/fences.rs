//! Gathers every code fence for the snippet lint and takes its hidden lines out
//! of the page.
//!
//! The lint reads what this records rather than the DOM, because by then the
//! lines it has to check are the ones this removed.

use typst_html::{HtmlDocument, HtmlElement, tag};

use crate::config::Config;
use crate::render::snippet::Snippet;

use super::{Cx, DocumentExt, Exempt, Highlight, Transform};

/// The [`Transform`] that collects fences and hides their marked lines.
pub(super) struct Fences;

impl Transform for Fences {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME, Highlight::NAME]
    }

    /// Runs for the languages `check { snippets { } }` names, which is where a
    /// hidden marker is declared and what the lint has to be handed.
    fn enabled(&self, config: &Config) -> bool {
        !config.check.snippets.is_empty()
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut fences = Vec::new();
        doc.walk(|element| {
            if element.tag == tag::code
                && let Some(snippet) = Self::gather(element, cx.config)
            {
                fences.push(snippet);
            }
        });
        cx.fences = fences;
    }
}

impl Fences {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "fences";

    /// The fence `element` is, hidden lines already taken out of it.
    fn gather(element: &mut HtmlElement, config: &Config) -> Option<Snippet> {
        let lang = element.attrs.get(crate::world::rules::LANG)?;
        let marker = config
            .check
            .snippet(lang)
            .and_then(|rule| rule.hidden.as_deref());
        let snippet = Snippet::of(element, marker)?;

        if snippet.hides() {
            snippet.hide(element);
        }
        Some(snippet)
    }
}
