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

    #[error("unknown starter template `{name}`")]
    #[diagnostic(code(baudelaire::scaffold::unknown_template), help("{help}"))]
    UnknownTemplate { name: String, help: String },
}

impl ScaffoldError {
    /// `help` comes from the same table that lists the templates, so a typo's
    /// suggestion can never name one that does not exist.
    pub fn unknown_template(name: &str, help: String) -> Self {
        Self::UnknownTemplate {
            name: name.to_owned(),
            help,
        }
    }

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
