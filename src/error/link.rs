//! Broken internal link reporting.
//!
//! Each broken `.typ` link is a related sub-diagnostic carrying the offending
//! page's source and a labeled span at the link target, so miette underlines the
//! exact reference — not just a flat list of strings.

use std::fmt;
use std::path::Path;

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceCode, SourceSpan};

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
/// `strict_links`; otherwise each is reported as a warning.
#[derive(Debug)]
pub struct BrokenLinks {
    links: Vec<Broken>,
}

impl BrokenLinks {
    pub fn new(links: Vec<Broken>) -> Self {
        Self { links }
    }
}

impl fmt::Display for BrokenLinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.links.len();
        write!(f, "found {n} broken internal link{}", if n == 1 { "" } else { "s" })
    }
}

impl std::error::Error for BrokenLinks {}

impl Diagnostic for BrokenLinks {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::links::broken"))
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(
            "every `.typ` link must resolve to an existing page; \
             pass `--strict-links false` to downgrade these to warnings",
        ))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
        Some(Box::new(self.links.iter().map(|l| l as &dyn Diagnostic)))
    }
}
