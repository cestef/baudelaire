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

    #[error("invalid date `{input}`")]
    #[diagnostic(
        code(baudelaire::scaffold::bad_date),
        help("expected `YYYY-MM-DD`, e.g. 2026-07-13")
    )]
    BadDate { input: String },
}

impl ScaffoldError {
    pub fn already_exists(path: &std::path::Path) -> Self {
        Self::AlreadyExists {
            path: path.display().to_string(),
        }
    }

    pub fn bad_date(input: &str) -> Self {
        Self::BadDate {
            input: input.to_owned(),
        }
    }
}

impl From<ScaffoldError> for crate::error::BaudelaireErrorKind {
    fn from(e: ScaffoldError) -> Self {
        Self::Scaffold(Box::new(e))
    }
}
