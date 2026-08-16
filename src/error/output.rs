//! Errors from the post-build processors that emit derived site files.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// An opt-in output was enabled on a site with no `url`, an error rather than a
/// warning because a warning let the build succeed with the output silently
/// missing.
#[derive(Debug, Error, Diagnostic)]
#[error("{} needs a site `url`", Code(.feature))]
#[diagnostic(
    code(baudelaire::output::url_required),
    help("set `url \"https://example.com\"`, or turn {} off", Code(.feature))
)]
pub struct BaseUrlRequired {
    pub feature: &'static str,
}
