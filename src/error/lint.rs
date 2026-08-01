//! Findings of the lint pass over the built pages.
//!
//! A [`Lint`] is what a rule found; a [`Flaw`] is that finding rendered against
//! the source it came from, with the offending bytes underlined. The rules
//! themselves live in [`crate::render::lint`], which produces the [`Lint`]s
//! while it walks the typed DOM: this module knows only how to say them.
//!
//! Both halves follow [`super::link`]: one diagnostic per finding, gathered
//! into one parent so a page with twenty missing `alt`s is one report and not
//! twenty.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use miette::{Diagnostic, LabeledSpan, NamedSource, Severity, SourceCode, SourceSpan};
use serde::{Deserialize, Serialize};

use crate::render::Site;
use crate::ui::{Bytes, Code, Text};

/// What a lint rule found. One variant per rule, carrying exactly what the
/// message needs, so the wording is written once here rather than assembled at
/// each rule.
///
/// Serialized as part of a page's cached outputs, so a cache hit reports what
/// it found when it was built.
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
}

impl Lint {
    /// The diagnostic code, one per rule so a report can be grepped and a CI
    /// gate can name what it is failing on.
    fn code(&self) -> &'static str {
        match self {
            Self::Heading { .. } => "baudelaire::lint::heading",
            Self::Alt => "baudelaire::lint::alt",
            Self::Id(_) => "baudelaire::lint::id",
            Self::Role(_) => "baudelaire::lint::role",
            Self::Attr(_) => "baudelaire::lint::attr",
            Self::Idref { .. } => "baudelaire::lint::idref",
        }
    }

    /// What to underline in the source. Constant, never interpolated: a label
    /// is not markup-rendered, so a value carrying a backtick would land raw in
    /// the output. What the value *is* belongs in the message.
    fn label(&self) -> &'static str {
        match self {
            Self::Heading { .. } => "this heading skips a level",
            Self::Alt => "no alt text",
            Self::Id(_) => "already used above",
            Self::Role(_) => "not an ARIA role",
            Self::Attr(_) => "not an ARIA attribute",
            Self::Idref { .. } => "nothing on this page has that id",
        }
    }

    /// What to do about it.
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
        }
    }
}

/// One finding, located in the source that produced it.
#[derive(Debug)]
pub struct Flaw {
    /// The page it was found on, relative to the content root.
    page: String,
    lint: Lint,
    /// The source file, for rendering the offending line.
    src: NamedSource<String>,
    /// Byte span of the element within that file, if it could be located: an
    /// element this crate synthesized belongs to no `.typ` at all.
    span: Option<SourceSpan>,
    /// Error under `lint { strict }`, warning otherwise; set by the [`Flaws`]
    /// constructor so parent and children render alike.
    severity: Severity,
}

impl Flaw {
    /// Build a finding's diagnostic against `source`, the text of the file it
    /// points into ([`Sources`] reads those, once each).
    ///
    /// The span is kept only when it actually lies within that text. A page's
    /// elements do not all come from a file on disk: a templated page compiles
    /// through a generated wrapper, and a listing page's source is synthesized
    /// entirely. Handing miette a span into text it does not have prints
    /// `[Failed to read contents for label]` where the source line belongs, so
    /// a finding with nothing to underline says only what it found and where.
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
            src: NamedSource::new(name, text).with_language("Typst"),
            span,
            severity: Severity::Warning,
        }
    }
}

/// The source files a batch of findings points into, read once each.
///
/// A page with twenty missing `alt`s is twenty findings in one file, and every
/// one of them wants that file's text to underline a line of. Without this the
/// file is read (and copied) twenty times.
#[derive(Default)]
pub struct Sources {
    /// Project-relative path -> its text, `None` for a file that is not on
    /// disk. The absences are cached too: the same synthesized path recurs for
    /// every finding on that page.
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

/// Everything the lint pass found. An error under `lint { strict }`
/// ([`Flaws::new`]); otherwise the identical report as a warning
/// ([`Flaws::warning`]).
#[derive(Debug)]
pub struct Flaws {
    flaws: Vec<Flaw>,
    severity: Severity,
}

impl Flaws {
    /// The strict form: a build-failing error.
    pub fn new(mut flaws: Vec<Flaw>) -> Self {
        for flaw in &mut flaws {
            flaw.severity = Severity::Error;
        }
        Self {
            flaws,
            severity: Severity::Error,
        }
    }

    /// The lenient form: the identical diagnostic at warning severity.
    pub fn warning(flaws: Vec<Flaw>) -> Self {
        Self {
            flaws,
            severity: Severity::Warning,
        }
    }
}

impl fmt::Display for Flaws {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.flaws.len();
        write!(f, "found {n} lint finding{}", if n == 1 { "" } else { "s" })
    }
}

impl std::error::Error for Flaws {}

impl Diagnostic for Flaws {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::lint::found"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(match self.severity {
            Severity::Error => {
                "fix each one, turn the rule off by name under `lint { }`, or \
                 set `lint { strict #false }` to downgrade these to warnings"
            }
            _ => "fix each one, or set `lint { strict }` to make them fail the build",
        }))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
        Some(Box::new(self.flaws.iter().map(|f| f as &dyn Diagnostic)))
    }
}

#[cfg(test)]
mod tests {
    use miette::Diagnostic as _;

    use super::{Flaw, Lint};
    use crate::render::Site;

    fn at(offset: usize, len: usize) -> Site {
        Site {
            file: "content/a.typ".into(),
            offset,
            len,
        }
    }

    /// A finding whose element came from a source this build cannot open (a
    /// generated listing, a page compiled through its wrapper) still reports;
    /// it simply underlines nothing.
    #[test]
    fn a_finding_with_no_readable_source_carries_no_span() {
        let flaw = Flaw::new("a.typ".into(), Lint::Alt, Some(&at(0, 4)), None);
        assert!(flaw.labels().is_none());
        assert!(flaw.source_code().is_none());
    }

    /// Nor does one whose span does not fit the text it names: miette would
    /// print a read failure where the source line belongs.
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
    /// The page, relative to the content root.
    pub page: String,
    /// Which budget it broke, as spelled under `lint { budget { } }`.
    pub budget: &'static str,
    pub weighed: Bytes,
    pub allowed: Bytes,
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
}

/// The pages that broke a weight budget. Always an error: a budget is a limit
/// the author wrote down, not an opinion this tool holds.
#[derive(Debug)]
pub struct Overweights(Vec<Overweight>);

impl From<Vec<Overweight>> for Overweights {
    fn from(over: Vec<Overweight>) -> Self {
        Self(over)
    }
}

impl fmt::Display for Overweights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.0.len();
        write!(f, "{n} page{} over budget", if n == 1 { "" } else { "s" })
    }
}

impl std::error::Error for Overweights {}

impl Diagnostic for Overweights {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::lint::budget"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(
            "ship less, or raise the limit under `lint { budget { } }`",
        ))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
        Some(Box::new(self.0.iter().map(|o| o as &dyn Diagnostic)))
    }
}
