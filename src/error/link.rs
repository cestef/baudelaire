//! Broken internal link reporting.
//!
//! Each broken `.typ` link is a related sub-diagnostic carrying the offending
//! page's source and a labeled span at the link target, so miette underlines the
//! exact reference, not just a flat list of strings.

use std::fmt;
use std::path::Path;

use miette::{Diagnostic, LabeledSpan, NamedSource, Severity, SourceCode, SourceSpan};

use crate::ui::{Code, Text};

/// Why a link did not resolve. One error class, two precise reasons: both are
/// located in the source and reported the same way, and only the wording and
/// the diagnostic code differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Miss {
    /// No page sits at the `.typ` path.
    Page,
    /// The page exists, but exposes no such heading id. Carries the fragment,
    /// since the raw target is the whole `path.typ#fragment`.
    Anchor(String),
}

/// An internal `.typ` link that does not resolve.
#[derive(Debug, Clone)]
pub struct Broken {
    /// The page containing the link, relative to the content root.
    pub page: String,
    /// The raw link target as authored.
    pub target: String,
    /// The page source, for rendering the offending line.
    src: NamedSource<String>,
    /// Byte span of the target within the source, if it could be located.
    span: Option<SourceSpan>,
    /// Error under `strict_links`, warning otherwise; set by the
    /// [`BrokenLinks`] constructor so parent and children render alike.
    severity: Severity,
    /// What was missing.
    miss: Miss,
}

impl Broken {
    /// Build a broken-link diagnostic, locating `target` within the page source
    /// so miette can underline it. `source` is the raw file text (empty for
    /// generated pages, whose sources never touch disk).
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
        let src = NamedSource::new(&page, text).with_language("Typst");
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

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let span = self.span?;
        // Constant, not interpolated: a label is not markup-rendered, so a
        // fragment carrying a backtick would land raw in the output. Which
        // heading is missing is already in the message, through `Code`.
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

/// The set of broken internal links found in a build. Raised as an error under
/// `strict_links` ([`BrokenLinks::new`]); otherwise collected as a warning
/// ([`BrokenLinks::warning`]) with the same spans and detail.
#[derive(Debug)]
pub struct BrokenLinks {
    links: Vec<Broken>,
    severity: Severity,
}

impl BrokenLinks {
    /// The strict-mode form: a build-failing error.
    pub fn new(links: Vec<Broken>) -> Self {
        Self {
            links,
            severity: Severity::Error,
        }
    }

    /// The lenient form: the identical diagnostic at warning severity, children
    /// included.
    pub fn warning(mut links: Vec<Broken>) -> Self {
        for link in &mut links {
            link.severity = Severity::Warning;
        }
        Self {
            links,
            severity: Severity::Warning,
        }
    }
}

impl fmt::Display for BrokenLinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.links.len();
        write!(
            f,
            "found {n} broken internal link{}",
            if n == 1 { "" } else { "s" }
        )
    }
}

impl std::error::Error for BrokenLinks {}

impl Diagnostic for BrokenLinks {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::links::broken"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(match self.severity {
            Severity::Error => {
                "every `.typ` link must resolve to an existing page; \
                 pass `--no-strict-links` to downgrade these to warnings"
            }
            _ => {
                "fix each target, or leave `--strict-links` on to make these \
                 fail the build"
            }
        }))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
        Some(Box::new(self.links.iter().map(|l| l as &dyn Diagnostic)))
    }
}

/// An outbound link whose host answered, and said no.
#[derive(Debug)]
pub struct Dead {
    pub url: String,
    pub status: u16,
    /// Every page linking to it, so one report names every place to fix.
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

/// Outbound links that answered with an error status. Raised by
/// `check --external`, where reaching the network is what was asked for.
#[derive(Debug)]
pub struct DeadLinks(Vec<Dead>);

impl From<Vec<Dead>> for DeadLinks {
    fn from(links: Vec<Dead>) -> Self {
        Self(links)
    }
}

impl fmt::Display for DeadLinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.0.len();
        write!(
            f,
            "found {n} dead outbound link{}",
            if n == 1 { "" } else { "s" }
        )
    }
}

impl std::error::Error for DeadLinks {}

impl Diagnostic for DeadLinks {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::links::dead"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(
            "update or remove each target; a permanent redirect is fine, \
             a 404 is not",
        ))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
        Some(Box::new(self.0.iter().map(|l| l as &dyn Diagnostic)))
    }
}

/// A page no other page's content links to.
///
/// Not an error and not a span: the problem is an *absence*, so there is nothing
/// in the page to underline. It names where the page is and where it is served,
/// which is what an author needs to decide whether it wants linking or wants
/// deleting.
#[derive(Debug, thiserror::Error, Diagnostic)]
#[error("{} is linked from nowhere, and serves at {}", Code(.page), Code(.url))]
#[diagnostic(code(baudelaire::links::orphan), severity(warning))]
pub struct Orphan {
    /// The page, relative to the content root.
    pub page: String,
    /// Its permalink.
    pub url: String,
}

/// The pages a reader can only reach by knowing the URL.
///
/// A report rather than a gate: a landing page linked from a hand-written nav is
/// an orphan by this definition and a perfectly ordinary thing to have, so it is
/// on the author to read the list. Generated listings and the not-found page are
/// left out, since nobody forgot to link those.
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
