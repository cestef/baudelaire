use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// Failures of `baudelaire packages`, which mirrors the generated
/// `@baudelaire/*` modules onto disk for editor tooling.
#[derive(Error, Diagnostic, Debug)]
pub enum PackagesError {
    #[error("no typst package directory on this platform")]
    #[diagnostic(
        code(baudelaire::packages::no_directory),
        help("name one yourself: {}", Code(&"baudelaire packages --path <dir>"))
    )]
    NoDirectory,
}

impl From<PackagesError> for crate::error::BaudelaireErrorKind {
    fn from(e: PackagesError) -> Self {
        Self::Packages(Box::new(e))
    }
}
