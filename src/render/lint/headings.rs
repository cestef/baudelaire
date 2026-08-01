//! Heading levels that skip.
//!
//! A document outline is read by level: assistive technology jumps `h2` to
//! `h2`, and a section introduced by an `h4` under an `h2` has no place in that
//! outline. Only *downward* skips are reported; coming back up any number of
//! levels closes sections and is how a document ends a chapter.

use crate::config::LintConfig;
use crate::error::Lint;

use super::{Findings, Page, Rule};

/// The rule that reports a heading level jumping by more than one.
pub(super) struct Headings;

impl Rule for Headings {
    fn enabled(&self, config: &LintConfig) -> bool {
        config.headings
    }

    fn check(&self, page: &Page, found: &mut Findings<'_>) {
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
