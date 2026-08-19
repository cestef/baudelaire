//! Findings of the lint pass over the built pages: a [`Lint`] is what a rule
//! found, a [`Flaw`] is that finding rendered against the source it came from.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use miette::{Diagnostic, LabeledSpan, NamedSource, Severity, SourceCode, SourceSpan};
use serde::{Deserialize, Serialize};

use super::aggregate::{Aggregate, Finding, Kind};
use crate::render::Site;
use crate::ui::{Bytes, Code, Text};

/// What a lint rule found; serialized with a page's cached outputs, so a cache
/// hit reports what it found when it was built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lint {
    /// A heading that skips a level: `h{from}` straight to `h{to}`.
    Heading { from: u8, to: u8 },
    /// An image carrying no `alt` attribute at all.
    Alt,
    /// An `id` that appears more than once on the page.
    Id(String),
    /// A `role` that is not an ARIA role.
    Role(String),
    /// An `aria-*` attribute that ARIA does not define.
    Attr(String),
    /// An id-referencing ARIA attribute naming an id the page does not have.
    Idref { attr: String, id: String },
    /// What a code fence's checker objected to. `message` is that checker's own
    /// words, escaped rather than read as markup.
    Snippet { lang: String, message: String },
}

impl Lint {
    /// Resolved at report time rather than cached with the finding, so a cache
    /// hit reports at the severity the *current* config asks for.
    pub fn ruled(&self) -> crate::config::Ruled {
        use crate::config::{Rule, Ruled};
        match self {
            Self::Heading { .. } => Rule::Headings.into(),
            Self::Alt => Rule::Alt.into(),
            Self::Id(_) => Rule::Ids.into(),
            Self::Role(_) | Self::Attr(_) | Self::Idref { .. } => Rule::Aria.into(),
            Self::Snippet { lang, .. } => Ruled::snippet(lang),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Heading { .. } => "baudelaire::lint::heading",
            Self::Alt => "baudelaire::lint::alt",
            Self::Id(_) => "baudelaire::lint::id",
            Self::Role(_) => "baudelaire::lint::role",
            Self::Attr(_) => "baudelaire::lint::attr",
            Self::Idref { .. } => "baudelaire::lint::idref",
            Self::Snippet { .. } => "baudelaire::lint::snippet",
        }
    }

    /// What to underline in the source. Constant, never interpolated: a label
    /// is not markup-rendered, so a value carrying a backtick would land raw.
    fn label(&self) -> &'static str {
        match self {
            Self::Heading { .. } => "this heading skips a level",
            Self::Alt => "no alt text",
            Self::Id(_) => "already used above",
            Self::Role(_) => "not an ARIA role",
            Self::Attr(_) => "not an ARIA attribute",
            Self::Idref { .. } => "nothing on this page has that id",
            Self::Snippet { .. } => "the checker stopped here",
        }
    }

    fn help(&self) -> &'static str {
        match self {
            Self::Heading { .. } => {
                "a screen reader navigates by heading level; give the section \
                 the next level down, or move it under a heading that fits"
            }
            Self::Alt => {
                "describe it: `image(\"photo.png\", alt: \"a cat\")`, or pass \
                 `alt: \"\"` to mark it decorative"
            }
            Self::Id(_) => {
                "an id names one element; a link to a repeated one reaches only \
                 the first"
            }
            Self::Role(_) => "check the spelling against the WAI-ARIA role list",
            Self::Attr(_) => "check the spelling against the WAI-ARIA attribute list",
            Self::Idref { .. } => {
                "point it at an element that is on this page, or drop the \
                 attribute"
            }
            Self::Snippet { .. } => {
                "fix the snippet, or take the language out of \
                 `lint { snippets }` if its fences are not meant to check"
            }
        }
    }
}

impl fmt::Display for Lint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Heading { from, to } => write!(
                f,
                "heading level jumps from {} to {}",
                Code(&format!("h{from}")),
                Code(&format!("h{to}"))
            ),
            Self::Alt => write!(f, "{} has no {}", Code("<img>"), Code("alt")),
            Self::Id(id) => write!(f, "the id {} is used more than once", Code(id)),
            Self::Role(role) => write!(f, "unknown ARIA role {}", Code(role)),
            Self::Attr(attr) => write!(f, "unknown ARIA attribute {}", Code(attr)),
            Self::Idref { attr, id } => write!(
                f,
                "{} names {}, which is not on this page",
                Code(attr),
                Code(&format!("#{id}"))
            ),
            Self::Snippet { lang, message } => {
                write!(f, "{} snippet: {}", Code(lang), Text(message))
            }
        }
    }
}

/// One finding, located in the source that produced it.
#[derive(Debug)]
pub struct Flaw {
    /// Relative to the content root.
    page: String,
    lint: Lint,
    src: NamedSource<String>,
    /// Byte span of the element within that file; `None` for an element this
    /// crate synthesized, which belongs to no `.typ` at all.
    span: Option<SourceSpan>,
    /// Error under `lint { strict }`, warning otherwise; set by the [`Flaws`]
    /// constructor so parent and children render alike.
    severity: Severity,
}

impl Flaw {
    /// The span is kept only when it lies within `source`: miette prints
    /// `[Failed to read contents for label]` for a span into text it lacks.
    pub fn new(page: String, lint: Lint, at: Option<&Site>, source: Option<&str>) -> Self {
        let (name, text, span) = match (at, source) {
            (Some(site), Some(text)) if site.offset + site.len <= text.len() => (
                site.file.clone(),
                text.to_owned(),
                Some(SourceSpan::new(site.offset.into(), site.len)),
            ),
            _ => (page.clone(), String::new(), None),
        };
        Self {
            page,
            lint,
            src: crate::error::PageSource(name, text).into(),
            span,
            severity: Severity::Warning,
        }
    }
}

/// The source files a batch of findings points into, read once each.
#[derive(Default)]
pub struct Sources {
    /// Project-relative path -> its text, `None` for a file that is not on
    /// disk.
    files: HashMap<String, Option<String>>,
}

impl Sources {
    /// The text of the file a finding points into, read on first ask.
    pub fn at(&mut self, at: Option<&Site>, root: &Path) -> Option<&str> {
        let site = at?;
        self.files
            .entry(site.file.clone())
            .or_insert_with(|| crate::fs::read_to_string(root.join(&site.file)).ok())
            .as_deref()
    }
}

impl fmt::Display for Flaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.lint, Text(&self.page))
    }
}

impl std::error::Error for Flaw {}

impl Diagnostic for Flaw {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(self.lint.code()))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(self.lint.help()))
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.span.map(|_| &self.src as &dyn SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let span = self.span?;
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            Some(self.lint.label().to_owned()),
            span,
        ))))
    }
}

/// An error under `lint { strict }`, otherwise the identical report as a
/// warning.
pub type Flaws = Aggregate<Flaw>;

impl Finding for Flaw {
    fn set_severity(&mut self, severity: Severity) {
        self.severity = severity;
    }
}

impl Flaws {
    const KIND: Kind = Kind {
        noun: ("lint finding", "lint findings"),
        code: "baudelaire::lint::found",
        strict: "fix each one, turn the rule off by name under `lint { }`, or \
                 set `lint { strict #false }` to downgrade these to warnings",
        lenient: Some("fix each one, or set `lint { strict }` to make them fail the build"),
    };

    pub fn new(flaws: Vec<Flaw>) -> Self {
        Self::at(flaws, Severity::Error, &Self::KIND)
    }

    pub fn warning(flaws: Vec<Flaw>) -> Self {
        Self::at(flaws, Severity::Warning, &Self::KIND)
    }
}

#[cfg(test)]
mod tests {
    use super::{Bytes, Overweight, Overweights};
    use miette::{Diagnostic as _, Severity};

    /// A child with no severity of its own renders as an error whatever its
    /// parent says, so a non-strict budget printed a warning headline over rows
    /// marked `error`.
    #[test]
    fn a_budget_child_renders_at_its_parents_severity() {
        let over = || {
            vec![Overweight::new(
                "posts/a.typ".to_owned(),
                "total",
                Bytes(2),
                Bytes(1),
            )]
        };
        for (aggregate, severity) in [
            (Overweights::new(over()), Severity::Error),
            (Overweights::warning(over()), Severity::Warning),
        ] {
            assert_eq!(aggregate.severity(), Some(severity));
            let children: Vec<_> = aggregate.related().expect("children").collect();
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].severity(), Some(severity));
        }
    }

    use super::{Flaw, Lint};
    use crate::render::Site;

    fn at(offset: usize, len: usize) -> Site {
        Site {
            file: "content/a.typ".into(),
            offset,
            len,
        }
    }

    #[test]
    fn a_finding_with_no_readable_source_carries_no_span() {
        let flaw = Flaw::new("a.typ".into(), Lint::Alt, Some(&at(0, 4)), None);
        assert!(flaw.labels().is_none());
        assert!(flaw.source_code().is_none());
    }

    #[test]
    fn a_span_past_the_end_of_the_source_is_dropped() {
        let flaw = Flaw::new("a.typ".into(), Lint::Alt, Some(&at(2, 900)), Some("short"));
        assert!(flaw.labels().is_none());

        let flaw = Flaw::new("a.typ".into(), Lint::Alt, Some(&at(0, 5)), Some("short"));
        assert!(flaw.labels().is_some());
    }
}

/// One page that ships more bytes than its budget allows.
#[derive(Debug)]
pub struct Overweight {
    /// Relative to the content root.
    pub page: String,
    /// As spelled under `lint { budget { } }`.
    pub budget: &'static str,
    pub weighed: Bytes,
    pub allowed: Bytes,
    /// Error under `lint { budget { strict } }`, warning otherwise; set by the
    /// [`Overweights`] constructor so parent and children render alike.
    severity: Severity,
}

impl Overweight {
    /// A warning until [`Overweights::new`] raises it, which is the default a
    /// child keeps when its parent is the non-strict aggregate.
    pub fn new(page: String, budget: &'static str, weighed: Bytes, allowed: Bytes) -> Self {
        Self {
            page,
            budget,
            weighed,
            allowed,
            severity: Severity::Warning,
        }
    }
}

impl fmt::Display for Overweight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} is {}, over the {} budget",
            Text(&self.page),
            Code(self.budget),
            Text(self.weighed),
            Text(self.allowed)
        )
    }
}

impl std::error::Error for Overweight {}

impl Diagnostic for Overweight {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::lint::budget"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }
}

/// An error by default, because a budget is a limit the author wrote down
/// rather than an opinion this tool holds.
pub type Overweights = Aggregate<Overweight>;

impl Finding for Overweight {
    fn set_severity(&mut self, severity: Severity) {
        self.severity = severity;
    }
}

impl Overweights {
    const KIND: Kind = Kind {
        noun: ("page over budget", "pages over budget"),
        code: "baudelaire::lint::budget",
        strict: "ship less, or raise the limit under `lint { budget { } }`",
        lenient: None,
    };

    pub fn new(over: Vec<Overweight>) -> Self {
        Self::at(over, Severity::Error, &Self::KIND)
    }

    /// For `lint { budget { strict #false } }`.
    pub fn warning(over: Vec<Overweight>) -> Self {
        Self::at(over, Severity::Warning, &Self::KIND)
    }
}
