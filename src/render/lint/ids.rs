//! Ids used more than once.
//!
//! An id names one element. A repeated one silently breaks every deep link and
//! every `aria-labelledby` into it, since both reach only the first, and the
//! anchor pass will not save it: a *derived* heading id is deduplicated against
//! everything already on the page, but two authored ones are the author's.
//!
//! Reported at the second and later occurrences, so the first stays the one
//! that owns the name.

use std::collections::HashSet;

use crate::config::LintConfig;
use crate::error::Lint;

use super::{Findings, Page, Rule};

/// The rule that reports a duplicate `id`.
pub(super) struct Ids;

impl Rule for Ids {
    fn enabled(&self, config: &LintConfig) -> bool {
        config.ids.on()
    }

    fn check(&self, page: &Page, found: &mut Findings<'_>) {
        let mut seen: HashSet<&str> = HashSet::new();
        for (id, span) in &page.ids {
            if !seen.insert(id.as_str()) {
                found.push(*span, Lint::Id(id.clone()));
            }
        }
    }
}
