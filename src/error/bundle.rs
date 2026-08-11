//! Failures while writing a bound document.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// A bundle whose container could not be written.
///
/// Fatal, like every other artifact this build promises: a site that asked for
/// a book and got a green build without one has no way to notice.
#[derive(Debug, Error, Diagnostic)]
pub enum BundleError {
    /// The zip writer failed. Kept whole rather than flattened to its message:
    /// `zip` is a direct dependency of the feature that writes this, so its
    /// error is nameable here, and a caller that wants the cause can reach it
    /// instead of parsing a string this crate wrote.
    #[cfg(feature = "epub")]
    #[error("the EPUB for {} could not be written", Code(.bundle))]
    #[diagnostic(code(baudelaire::bundle::epub))]
    Epub {
        bundle: String,
        #[source]
        source: zip::result::ZipError,
    },
}

impl BundleError {
    #[cfg(feature = "epub")]
    pub fn epub(bundle: &str, source: impl Into<zip::result::ZipError>) -> Self {
        Self::Epub {
            bundle: bundle.to_owned(),
            source: source.into(),
        }
    }
}
