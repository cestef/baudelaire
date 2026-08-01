//! Images with no text alternative.
//!
//! A missing `alt` and an empty one are different claims: an empty one says the
//! image is decorative and the page reads the same without it, which only the
//! author can know. Absent means nobody decided, so it is what is reported. An
//! image already kept out of the accessibility tree (`aria-hidden`,
//! `role="presentation"`) has made that decision and is left alone; the model
//! filters those out while it gathers.

use crate::config::LintConfig;
use crate::error::Lint;

use super::{Findings, Page, Rule};

/// The rule that reports an `<img>` carrying no `alt`.
pub(super) struct Alt;

impl Rule for Alt {
    fn enabled(&self, config: &LintConfig) -> bool {
        config.alt
    }

    fn check(&self, page: &Page, found: &mut Findings<'_>) {
        for &span in &page.unlabelled {
            found.push(span, Lint::Alt);
        }
    }
}
