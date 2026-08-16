//! Failures while writing a bound document.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// A bundle whose container could not be written, fatal like every other
/// artifact this build promises.
#[derive(Debug, Error, Diagnostic)]
pub enum BundleError {
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
