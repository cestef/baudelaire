//! Many findings of one kind reported as one diagnostic, the findings its
//! children.

use std::fmt;

use miette::{Diagnostic, Severity};

/// What one aggregate says about itself, stated once per kind.
pub struct Kind {
    /// Counted in the headline: the singular, then the plural.
    pub noun: (&'static str, &'static str),
    /// What a user greps by, and so distinct per kind.
    pub code: &'static str,
    /// The help when the aggregate fails the build.
    pub strict: &'static str,
    /// The help when it does not, `None` where the same advice applies.
    pub lenient: Option<&'static str>,
}

/// A finding that renders at its aggregate's severity, so a headline and the
/// rows beneath it cannot contradict each other.
///
/// The default does nothing, for a finding whose severity is fixed by its own
/// `#[diagnostic]`.
pub trait Finding {
    fn set_severity(&mut self, _severity: Severity) {}
}

/// Findings of one kind, headed by a count and carrying each finding as a
/// child.
///
/// The shape is written once because its parts drift when it is not: an
/// aggregate whose children do not share its severity renders rows that
/// contradict their own headline.
pub struct Aggregate<T> {
    items: Vec<T>,
    severity: Severity,
    kind: &'static Kind,
}

impl<T: Finding> Aggregate<T> {
    /// The findings at `severity`, every child set to match, so a headline and
    /// the rows beneath it cannot contradict each other.
    pub fn at(mut items: Vec<T>, severity: Severity, kind: &'static Kind) -> Self {
        for item in &mut items {
            item.set_severity(severity);
        }
        Self {
            items,
            severity,
            kind,
        }
    }
}

impl<T> fmt::Debug for Aggregate<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Aggregate")
            .field("items", &self.items)
            .field("severity", &self.severity)
            .field("code", &self.kind.code)
            .finish()
    }
}

impl<T> fmt::Display for Aggregate<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.items.len();
        let (singular, plural) = self.kind.noun;
        write!(f, "found {n} {}", if n == 1 { singular } else { plural })
    }
}

impl<T: fmt::Debug> std::error::Error for Aggregate<T> {}

impl<T: Diagnostic + 'static> Diagnostic for Aggregate<T> {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(self.kind.code))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(match self.severity {
            Severity::Error => self.kind.strict,
            _ => self.kind.lenient.unwrap_or(self.kind.strict),
        }))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &dyn Diagnostic> + '_>> {
        Some(Box::new(
            self.items.iter().map(|item| item as &dyn Diagnostic),
        ))
    }
}
