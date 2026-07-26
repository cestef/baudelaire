//! Errors from the post-build processors that emit derived site files.

use miette::Diagnostic;
use thiserror::Error;

/// An opt-in output was enabled on a site with no `url`.
///
/// Absolute URLs are not optional in a sitemap or a feed, so there is nothing
/// to emit. An error rather than a warning because the feature was asked for:
/// warning let a build "succeed" with the feed silently missing.
#[derive(Debug, Error, Diagnostic)]
#[error("`{feature}` needs a site `url`")]
#[diagnostic(
    code(baudelaire::output::url_required),
    help("set `url \"https://example.com\"`, or turn `{feature}` off")
)]
pub struct BaseUrlRequired {
    pub feature: &'static str,
}
