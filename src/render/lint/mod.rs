//! Lint rules over the typed HTML DOM.
//!
//! Every rule reads the DOM typst-html handed back, not a re-parse of the
//! serialized page, so a finding is anchored at the `.typ` bytes that wrote it
//! rather than at a byte offset into generated markup. That is the whole reason
//! these live here and not in a downstream tool.
//!
//! [`Rules::builtin`] is the single source of what runs: a new rule is one
//! `impl Rule` in a module of its own plus one line in that list, each gated on
//! its own `lint { }` key. The DOM is walked exactly once, by [`Page::of`];
//! rules judge what that walk gathered, so adding one costs no second traversal
//! and a rule can read what another element carried (the `aria` rule reads the
//! page's whole id set).
//!
//! Weighing a page against `lint { budget { } }` is [`weigh`]'s, next door: it
//! produces a ledger rather than findings, and is the one part of this pass a
//! site-wide check finishes off.

mod alt;
mod aria;
mod headings;
mod ids;
mod weigh;

pub use weigh::{Load, Reference, Weight};

use typst::syntax::Span;
use typst_html::{HtmlDocument, HtmlElement, attr, tag};

use crate::config::LintConfig;
use crate::error::Lint;
use crate::world::PageWorld;

use super::origin::{Origins, Site};
use super::transform::{DocumentExt, ElementExt};

/// One thing a rule found, and where in the project it was written. `at` is
/// absent for an element this crate synthesized, which belongs to no `.typ`.
///
/// Stored in the page's cached outputs, so a cache hit reports what the build
/// that compiled it found. Deriving `Serialize` here rather than rebuilding the
/// diagnostic from the markup is deliberate: the markup no longer knows which
/// source line it came from.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub lint: Lint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Site>,
}

/// The findings of one page, and the resolver that locates them.
///
/// A rule pushes a [`Lint`] with the span of the element it is complaining
/// about and never touches a file path: the resolution is done here, once, so
/// no rule can forget it or spell a location differently.
pub struct Findings<'a> {
    origins: Origins<'a>,
    out: Vec<Finding>,
}

impl Findings<'_> {
    /// Record `lint`, anchored at the source `span` names.
    fn push(&mut self, span: Span, lint: Lint) {
        let at = self.origins.site(span);
        self.out.push(Finding { lint, at });
    }
}

/// What one walk of a page's DOM gathered, in document order. Every rule reads
/// this rather than the document, which is what keeps the walk single and lets
/// a rule see the page as a whole (a duplicate id, an ARIA reference to an
/// element written a thousand lines later).
pub struct Page {
    /// Every heading, as `(level, span)`.
    headings: Vec<(u8, Span)>,
    /// Every `id` on the page, in the order they appear.
    ids: Vec<(String, Span)>,
    /// Images carrying no `alt` attribute, and not marked decorative.
    unlabelled: Vec<Span>,
    /// Every `role` value, with the element that carried it.
    roles: Vec<(String, Span)>,
    /// Every `aria-*` attribute, as `(name, value, span)`.
    aria: Vec<(String, String, Span)>,
}

impl Page {
    /// Gather `doc` in one pass.
    fn of(doc: &HtmlDocument) -> Self {
        let mut page = Self {
            headings: Vec::new(),
            ids: Vec::new(),
            unlabelled: Vec::new(),
            roles: Vec::new(),
            aria: Vec::new(),
        };
        doc.visit(|element| page.visit(element));
        page
    }

    /// Read one element into the model.
    fn visit(&mut self, element: &HtmlElement) {
        let span = element.span;
        if let Some(level) = element.heading() {
            self.headings.push((level, span));
        }
        if let Some(id) = element.attrs.get(attr::id) {
            self.ids.push((id.to_string(), span));
        }
        if element.tag == tag::img
            && element.attrs.get(attr::alt).is_none()
            && !Self::decorative(element)
        {
            self.unlabelled.push(span);
        }
        for (key, value) in &element.attrs.0 {
            let name = key.resolve();
            match name.as_str() {
                "role" => self.roles.push((value.to_string(), span)),
                name if name.starts_with("aria-") => {
                    self.aria.push((name.to_owned(), value.to_string(), span));
                }
                _ => {}
            }
        }
    }

    /// Whether an element is explicitly not for the accessibility tree, and so
    /// wants no label: an icon beside text that already says the same thing.
    fn decorative(element: &HtmlElement) -> bool {
        element
            .attrs
            .get(attr::aria_hidden)
            .is_some_and(|v| v == "true")
            || element
                .attrs
                .get(attr::role)
                .is_some_and(|v| v == "presentation" || v == "none")
    }

    /// The page's id set, for a rule resolving an id reference.
    fn has(&self, id: &str) -> bool {
        self.ids.iter().any(|(seen, _)| seen == id)
    }
}

/// One lint rule.
pub(super) trait Rule: Send + Sync {
    /// Whether to run, from config alone: the same declarative gate a
    /// [`super::transform::Transform`] carries.
    fn enabled(&self, config: &LintConfig) -> bool;
    /// Judge the gathered page, recording what it finds.
    fn check(&self, page: &Page, found: &mut Findings<'_>);
}

/// The built-in rules, in report order. THE single source of what runs.
pub(super) struct Rules(Vec<Box<dyn Rule>>);

impl Rules {
    pub(super) fn builtin() -> Self {
        Self(vec![
            Box::new(headings::Headings),
            Box::new(alt::Alt),
            Box::new(ids::Ids),
            Box::new(aria::Aria),
        ])
    }

    /// Lint `doc` and weigh it, returning both. `world` is the one the page
    /// compiled in, so a finding can be traced back to the source it names.
    ///
    /// The weighing is unconditional once the pass runs: it is one walk, and a
    /// budget is checked site-wide afterwards, where the sizes of the files a
    /// page loads are known.
    pub(super) fn run(
        &self,
        doc: &HtmlDocument,
        config: &LintConfig,
        world: &PageWorld,
    ) -> (Vec<Finding>, Weight) {
        let page = Page::of(doc);
        let mut found = Findings {
            origins: Origins::new(world),
            out: Vec::new(),
        };
        for rule in &self.0 {
            if rule.enabled(config) {
                rule.check(&page, &mut found);
            }
        }
        (found.out, Weight::of(doc))
    }
}
