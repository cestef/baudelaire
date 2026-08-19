//! Lint rules over the typed HTML DOM, one module each, gated on `check { }`.
//! [`Rules::builtin`] is the single source of what runs.

mod alt;
mod aria;
mod headings;
mod ids;
pub mod snippet;
mod weigh;

pub use weigh::{Load, Reference, Weight};

use typst::syntax::Span;
use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, attr, tag};

use crate::config::{CheckConfig, Named as _, Rule};
use crate::error::Lint;
use crate::world::PageWorld;

use super::origin::{Origins, Site};
use super::transform::{DocumentExt, ElementExt};

/// One thing a rule found, and where in the project it was written. `at` is
/// absent for an element this crate synthesized, which belongs to no `.typ`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub lint: Lint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Site>,
}

/// The findings of one page, and the resolver that locates them.
pub struct Findings<'a> {
    origins: Origins<'a>,
    /// What the author marked, and which rules each marker keeps off.
    exempt: &'a Exemptions,
    out: Vec<Finding>,
}

impl Findings<'_> {
    fn push(&mut self, span: Span, lint: Lint) {
        if self.exempt.covers(span, lint.ruled().rule) {
            return;
        }
        let at = self.origins.site(span);
        self.out.push(Finding { lint, at });
    }
}

/// Every marked span on one page, and what each marker keeps off.
#[derive(Debug, Default)]
pub struct Exemptions(std::collections::HashMap<Span, Exemption>);

impl Exemptions {
    /// Mark `span`, folding in whatever a marker around it already said.
    pub fn insert(&mut self, span: Span, exemption: &Exemption) {
        self.0.entry(span).or_default().merge(exemption);
    }

    /// Whether a finding of `rule` at `span` was asked for.
    fn covers(&self, span: Span, rule: Rule) -> bool {
        self.0.get(&span).is_some_and(|marked| marked.covers(rule))
    }
}

/// The attribute an author marks a region with to keep the lint off it, for the
/// finding that is right about the markup and wrong about the page.
///
/// Read and removed before the page is written: see
/// [`Exempt`](crate::render::transform).
pub const EXEMPT: HtmlAttr = HtmlAttr::constant("data-lint");

/// What a marker keeps off: every rule (`None`), or the ones it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exemption(Option<Vec<Rule>>);

/// Nothing, which is what a span no marker covers is owed.
impl Default for Exemption {
    fn default() -> Self {
        Self(Some(Vec::new()))
    }
}

impl Exemption {
    /// The value that means every rule, spelled as the config spells a rule
    /// that does not run.
    const ALL: &'static str = "off";

    /// What `value` exempts, or `None` when it names nothing this build has.
    ///
    /// Unknown names are dropped rather than refused: the marker is markup, and
    /// a page that names a rule this build has never heard of is still a page.
    pub fn parse(value: &str) -> Self {
        if value.split_whitespace().any(|word| word == Self::ALL) {
            return Self(None);
        }
        Self(Some(
            value.split_whitespace().filter_map(Rule::of).collect(),
        ))
    }

    /// Whether `rule` is one of the rules this keeps off.
    fn covers(&self, rule: Rule) -> bool {
        self.0.as_ref().is_none_or(|named| named.contains(&rule))
    }

    /// Both markers at once, for a region inside another.
    fn merge(&mut self, other: &Self) {
        match (&mut self.0, &other.0) {
            (_, None) => self.0 = None,
            (None, _) => {}
            (Some(named), Some(more)) => named.extend(more.iter().copied()),
        }
    }
}

/// What a lint rule is given besides the page: the config it answers to, the
/// project root a checker of its own runs in, and the fences the transform
/// pipeline gathered.
pub(super) struct Cx<'a> {
    pub(super) config: &'a CheckConfig,
    pub(super) root: &'a std::path::Path,
    pub(super) fences: &'a [crate::render::snippet::Snippet],
}

/// What one walk of a page's DOM gathered, in document order.
pub struct Page {
    /// Every heading, as `(level, span)`.
    headings: Vec<(u8, Span)>,
    ids: Vec<(String, Span)>,
    /// Images carrying no `alt` attribute, and not marked decorative.
    unlabelled: Vec<Span>,
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

    /// Whether an element is explicitly kept out of the accessibility tree, and
    /// so wants no label.
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

    fn has(&self, id: &str) -> bool {
        self.ids.iter().any(|(seen, _)| seen == id)
    }
}

pub(super) trait Check: Send + Sync {
    /// Whether to run, from config alone.
    fn enabled(&self, config: &CheckConfig) -> bool;
    /// Judge the gathered page, recording what it finds.
    fn check(&self, page: &Page, cx: &Cx<'_>, found: &mut Findings<'_>);
}

/// The built-in rules, in report order.
pub(super) struct Rules(Vec<Box<dyn Check>>);

impl Rules {
    pub(super) fn builtin() -> Self {
        Self(vec![
            Box::new(headings::Headings),
            Box::new(alt::Alt),
            Box::new(ids::Ids),
            Box::new(aria::Aria),
            Box::new(snippet::Snippets),
        ])
    }

    /// Lint `doc` and weigh it, returning both. `world` must be the one the
    /// page compiled in, so a finding can name the source it came from, `root`
    /// the project root a snippet checker runs in, and `fences` what the
    /// transform pipeline gathered for the snippet rule.
    pub(super) fn run(
        &self,
        doc: &HtmlDocument,
        config: &CheckConfig,
        world: &PageWorld,
        root: &std::path::Path,
        fences: &[crate::render::snippet::Snippet],
        exempt: &Exemptions,
    ) -> (Vec<Finding>, Weight) {
        let page = Page::of(doc);
        let cx = Cx {
            config,
            root,
            fences,
        };
        let mut found = Findings {
            origins: Origins::new(world),
            exempt,
            out: Vec::new(),
        };
        for rule in &self.0 {
            if rule.enabled(config) {
                rule.check(&page, &cx, &mut found);
            }
        }
        (found.out, Weight::of(doc))
    }
}
