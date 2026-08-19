//! Broken internal link reporting: one sub-diagnostic per broken `.typ` link,
//! carrying the offending page's source and a labeled span at the link target.

use std::fmt;
use std::path::Path;

use miette::{Diagnostic, LabeledSpan, NamedSource, Severity, SourceCode, SourceSpan};

use super::aggregate::{Aggregate, Finding, Kind};
use crate::ui::{Code, Text};

/// Why a link did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Miss {
    /// No page sits at the `.typ` path.
    Page,
    /// The page exists, but exposes no such heading id; carries the fragment,
    /// since the raw target is the whole `path.typ#fragment`.
    Anchor(String),
}

/// An internal `.typ` link that does not resolve.
#[derive(Debug, Clone)]
pub struct Broken {
    /// Relative to the content root.
    pub page: String,
    /// The raw link target as authored.
    pub target: String,
    src: NamedSource<String>,
    /// Byte span of the target within the source, `None` when it could not be
    /// located.
    span: Option<SourceSpan>,
    /// Error under `strict_links`, warning otherwise; set by the
    /// [`BrokenLinks`] constructor so parent and children render alike.
    severity: Severity,
    miss: Miss,
}

impl Broken {
    /// Locates `target` within the page's source so miette can underline it.
    pub fn new(page: String, target: String, source: &Path) -> Self {
        Self::missing(page, target, source, Miss::Page)
    }

    /// A link whose page resolved but whose `#fragment` names no heading there.
    pub fn anchor(page: String, target: String, source: &Path, fragment: String) -> Self {
        Self::missing(page, target, source, Miss::Anchor(fragment))
    }

    fn missing(page: String, target: String, source: &Path, miss: Miss) -> Self {
        let text = crate::fs::read_to_string(source).unwrap_or_default();
        let span = text
            .find(&target)
            .map(|offset| SourceSpan::new(offset.into(), target.len()));
        let src = crate::error::PageSource(page.clone(), text).into();
        Self {
            page,
            target,
            src,
            span,
            severity: Severity::Error,
            miss,
        }
    }
}

impl fmt::Display for Broken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.miss {
            Miss::Page => write!(f, "{} has no matching page", Code(&self.target)),
            Miss::Anchor(fragment) => write!(
                f,
                "{} has no heading {}",
                Code(&self.target),
                Code(&format!("#{fragment}"))
            ),
        }
    }
}

impl std::error::Error for Broken {}

impl Diagnostic for Broken {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(match self.miss {
            Miss::Page => "baudelaire::links::broken",
            Miss::Anchor(_) => "baudelaire::links::anchor",
        }))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.span.map(|_| &self.src as &dyn SourceCode)
    }

    /// The label is constant, never interpolated: a label is not
    /// markup-rendered, so a fragment carrying a backtick would land raw.
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let span = self.span?;
        let label = match self.miss {
            Miss::Page => "no page here",
            Miss::Anchor(_) => "no such heading on that page",
        };
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            Some(label.to_owned()),
            span,
        ))))
    }
}

/// An error under `strict_links`, otherwise the identical report as a warning.
pub type BrokenLinks = Aggregate<Broken>;

impl Finding for Broken {
    fn set_severity(&mut self, severity: Severity) {
        self.severity = severity;
    }
}

impl BrokenLinks {
    const KIND: Kind = Kind {
        noun: ("broken internal link", "broken internal links"),
        code: "baudelaire::links::broken",
        strict: "every `.typ` link must resolve to an existing page; \
                 pass `--no-strict-links` to downgrade these to warnings",
        lenient: Some(
            "fix each target, or leave `--strict-links` on to make these \
             fail the build",
        ),
    };

    pub fn new(links: Vec<Broken>) -> Self {
        Self::at(links, Severity::Error, &Self::KIND)
    }

    /// Children included.
    pub fn warning(links: Vec<Broken>) -> Self {
        Self::at(links, Severity::Warning, &Self::KIND)
    }
}

/// An outbound link whose host answered, and said no.
#[derive(Debug)]
pub struct Dead {
    pub url: String,
    pub status: u16,
    /// Every page linking to it.
    pub pages: Vec<String>,
}

impl fmt::Display for Dead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} -> HTTP {} (linked from {})",
            Text(&self.url),
            self.status,
            Text(self.pages.join(", "))
        )
    }
}

impl std::error::Error for Dead {}

impl Diagnostic for Dead {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::links::dead"))
    }
}

/// Outbound links that answered with an error status, from `check --external`.
pub type DeadLinks = Aggregate<Dead>;

impl Finding for Dead {}

impl DeadLinks {
    const KIND: Kind = Kind {
        noun: ("dead outbound link", "dead outbound links"),
        code: "baudelaire::links::dead",
        strict: "update or remove each target; a permanent redirect is fine, \
                 a 404 is not",
        lenient: None,
    };
}

impl From<Vec<Dead>> for DeadLinks {
    fn from(links: Vec<Dead>) -> Self {
        Self::at(links, Severity::Error, &Self::KIND)
    }
}

/// A page no other page's content links to. Carries no span: the problem is an
/// *absence*, so there is nothing in the page to underline.
#[derive(Debug, thiserror::Error, Diagnostic)]
#[error("{} is linked from nowhere, and serves at {}", Code(.page), Code(.url))]
#[diagnostic(code(baudelaire::links::orphan), severity(warning))]
pub struct Orphan {
    /// Relative to the content root.
    pub page: String,
    pub url: String,
}

/// A report rather than a gate: a landing page linked from a hand-written nav
/// is an orphan by this definition, and an ordinary thing to have.
#[derive(Debug, thiserror::Error, Diagnostic)]
#[error("{} linked from nowhere", crate::ui::Count::pages(.pages.len()))]
#[diagnostic(
    code(baudelaire::links::orphans),
    severity(warning),
    help("link each from a page that is reachable, or drop `links {{ orphans }}`")
)]
pub struct OrphanPages {
    #[related]
    pub pages: Vec<Orphan>,
}

impl From<Vec<Orphan>> for OrphanPages {
    fn from(pages: Vec<Orphan>) -> Self {
        Self { pages }
    }
}
