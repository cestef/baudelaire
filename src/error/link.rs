//! Broken internal link reporting.
//!
//! Each broken `.typ` link is a related sub-diagnostic carrying the offending
//! page's source and a labeled span at the link target, so miette underlines the
//! exact reference, not just a flat list of strings.

use std::fmt;
use std::path::Path;

use miette::{Diagnostic, LabeledSpan, NamedSource, Severity, SourceCode, SourceSpan};

/// An internal `.typ` link pointing at a page that does not exist.
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
}

impl Broken {
    /// Build a broken-link diagnostic, locating `target` within the page source
    /// so miette can underline it. `source` is the raw file text (empty for
    /// generated pages, whose sources never touch disk).
    pub fn new(page: String, target: String, source: &Path) -> Self {
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
        }
    }
}

impl fmt::Display for Broken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` has no matching page", self.target)
    }
}

impl std::error::Error for Broken {}

impl Diagnostic for Broken {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::links::broken"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.span.map(|_| &self.src as &dyn SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let span = self.span?;
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            Some("no page here".into()),
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
                 pass `--strict-links false` to downgrade these to warnings"
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
