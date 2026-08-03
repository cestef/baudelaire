use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// Failures of `baudelaire mirror`, which writes the generated modules onto
/// disk for editor tooling.
#[derive(Error, Diagnostic, Debug)]
pub enum MirrorError {
    #[error("no typst package directory on this platform")]
    #[diagnostic(
        code(baudelaire::mirror::directory),
        help("name one yourself: {}", Code(&"baudelaire mirror --path <dir>"))
    )]
    NoDirectory,
}

impl From<MirrorError> for crate::error::BaudelaireErrorKind {
    fn from(e: MirrorError) -> Self {
        Self::Mirror(Box::new(e))
    }
}
