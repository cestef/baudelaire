//! Ids used more than once, reported at the second and later occurrences so
//! the first stays the one that owns the name.

use std::collections::HashSet;

use crate::config::LintConfig;
use crate::error::Lint;

use super::{Check, Cx, Findings, Page};

/// The rule that reports a duplicate `id`.
pub(super) struct Ids;

impl Check for Ids {
    fn enabled(&self, config: &LintConfig) -> bool {
        config.ids.on()
    }

    fn check(&self, page: &Page, _cx: &Cx<'_>, found: &mut Findings<'_>) {
        let mut seen: HashSet<&str> = HashSet::new();
        for (id, span) in &page.ids {
            if !seen.insert(id.as_str()) {
                found.push(*span, Lint::Id(id.clone()));
            }
        }
    }
}
