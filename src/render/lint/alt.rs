//! Images with no text alternative: only an *absent* `alt` is reported, since
//! an empty one is the author's claim that the image is decorative.

use crate::config::LintConfig;
use crate::error::Lint;

use super::{Findings, Page, Rule};

/// The rule that reports an `<img>` carrying no `alt`.
pub(super) struct Alt;

impl Rule for Alt {
    fn enabled(&self, config: &LintConfig) -> bool {
        config.alt.on()
    }

    fn check(&self, page: &Page, found: &mut Findings<'_>) {
        for &span in &page.unlabelled {
            found.push(span, Lint::Alt);
        }
    }
}
