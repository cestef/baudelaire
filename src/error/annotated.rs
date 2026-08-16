//! A generic, self-contained miette diagnostic, the shape a dependency's own
//! diagnostic (kdl, wax, ..) is lowered into when it is built against an
//! incompatible miette version.

use std::fmt;
use std::ops::Range;

use miette::{Diagnostic, LabeledSpan, SourceCode};

/// A headline message over a snippet of source, with labeled byte-range spans
/// and an optional help line.
#[derive(Debug)]
pub struct Annotated {
    code: &'static str,
    message: String,
    source: String,
    help: Option<String>,
    labels: Vec<Label>,
}

#[derive(Debug)]
struct Label {
    message: String,
    span: Range<usize>,
}

impl Annotated {
    pub fn new(code: &'static str, message: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: source.into(),
            help: None,
            labels: Vec::new(),
        }
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Annotate `[offset, offset + len)` of the source with a message.
    pub fn label(mut self, message: impl Into<String>, offset: usize, len: usize) -> Self {
        self.labels.push(Label {
            message: message.into(),
            span: offset..offset + len,
        });
        self
    }
}

impl fmt::Display for Annotated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Annotated {}

impl Diagnostic for Annotated {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(self.code))
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h) as Box<dyn fmt::Display + '_>)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(&self.source)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if self.labels.is_empty() {
            return None;
        }
        Some(Box::new(self.labels.iter().map(|l| {
            LabeledSpan::new(Some(l.message.clone()), l.span.start, l.span.len())
        })))
    }
}
