//! Heading levels that skip. Only *downward* skips are reported; coming back
//! up any number of levels closes sections and is how a document ends a
//! chapter.

use crate::config::LintConfig;
use crate::error::Lint;

use super::{Check, Cx, Findings, Page};

/// The rule that reports a heading level jumping by more than one.
pub(super) struct Headings;

impl Check for Headings {
    fn enabled(&self, config: &LintConfig) -> bool {
        config.headings.on()
    }

    fn check(&self, page: &Page, _cx: &Cx<'_>, found: &mut Findings<'_>) {
        let mut previous: Option<u8> = None;
        for &(level, span) in &page.headings {
            if let Some(from) = previous
                && level > from + 1
            {
                found.push(span, Lint::Heading { from, to: level });
            }
            previous = Some(level);
        }
    }
}
