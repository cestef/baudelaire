use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum ScaffoldError {
    #[error("file already exists at `{path}`")]
    #[diagnostic(
        code(baudelaire::scaffold::already_exists),
        help("remove the file first, or choose a different path")
    )]
    AlreadyExists { path: String },
}

impl ScaffoldError {
    pub fn already_exists(path: &std::path::Path) -> Self {
        Self::AlreadyExists {
            path: path.display().to_string(),
        }
    }
}

impl From<ScaffoldError> for crate::error::BaudelaireErrorKind {
    fn from(e: ScaffoldError) -> Self {
        Self::Scaffold(Box::new(e))
    }
}
