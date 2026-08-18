//! Images with no text alternative: only an *absent* `alt` is reported, since
//! an empty one is the author's claim that the image is decorative.

use crate::config::LintConfig;
use crate::error::Lint;

use super::{Check, Cx, Findings, Page};

/// The rule that reports an `<img>` carrying no `alt`.
pub(super) struct Alt;

impl Check for Alt {
    fn enabled(&self, config: &LintConfig) -> bool {
        config.alt.on()
    }

    fn check(&self, page: &Page, _cx: &Cx<'_>, found: &mut Findings<'_>) {
        for &span in &page.unlabelled {
            found.push(span, Lint::Alt);
        }
    }
}
