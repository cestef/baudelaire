use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ScaffoldError {
    kind: ScaffoldErrorKind,
}

impl ScaffoldError {
    pub fn already_exists(path: &std::path::Path) -> Self {
        Self {
            kind: ScaffoldErrorKind::AlreadyExists {
                path: path.display().to_string(),
            },
        }
    }
}

impl miette::Diagnostic for ScaffoldError {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.code()
    }

    fn help(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.help()
    }
}

#[derive(Error, Diagnostic, Debug)]
pub enum ScaffoldErrorKind {
    #[error("file already exists at `{path}`")]
    #[diagnostic(
        code(baudelaire::scaffold::already_exists),
        help("remove the file first, or choose a different path")
    )]
    AlreadyExists { path: String },
}

impl From<ScaffoldError> for crate::error::BaudelaireErrorKind {
    fn from(e: ScaffoldError) -> Self {
        crate::error::BaudelaireErrorKind::Scaffold(Box::new(e))
    }
}
