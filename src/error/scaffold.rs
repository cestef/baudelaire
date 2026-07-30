use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

#[derive(Error, Diagnostic, Debug)]
pub enum ScaffoldError {
    #[error("file already exists at {}", Code(.path))]
    #[diagnostic(
        code(baudelaire::scaffold::already_exists),
        help("remove the file first, or choose a different path")
    )]
    AlreadyExists { path: String },

    #[error("invalid date {}", Code(.input))]
    #[diagnostic(
        code(baudelaire::scaffold::bad_date),
        help("expected `YYYY-MM-DD`, e.g. 2026-07-13")
    )]
    BadDate { input: String },

    #[error("unknown starter template {}", Code(.name))]
    #[diagnostic(code(baudelaire::scaffold::unknown_template), help("{help}"))]
    UnknownTemplate { name: String, help: String },

    #[error("unknown optional feature {}", Code(.name))]
    #[diagnostic(code(baudelaire::scaffold::unknown_extra), help("{help}"))]
    UnknownExtra { name: String, help: String },

    /// `--profile` is global, so it reaches `init` too, and `init` has nothing
    /// to apply it to. It used to be accepted and ignored: the run reported
    /// success having done none of what was asked.
    #[error("{} does not apply to {}", Code("--profile"), Code("baudelaire init"))]
    #[diagnostic(
        code(baudelaire::scaffold::profile),
        help("a profile narrows a config that already exists; `init` writes the first one")
    )]
    Profile,

    /// `--config` names where the scaffolded config lands, and only a filename
    /// can: every `paths { }` entry resolves against the working directory
    /// rather than against the config file, so a config nested a directory down
    /// would name a content tree outside the project.
    #[error("{} names a path, not a filename", Code(.path))]
    #[diagnostic(
        code(baudelaire::scaffold::config_path),
        help("`init` writes the config at the project root; pass a bare name like `site.kdl`")
    )]
    ConfigPath { path: String },
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

    /// The `--with` counterpart of [`Self::unknown_template`], its own class so
    /// the message names a feature rather than a starter template; `help` again
    /// comes from the table that defines what is valid.
    pub fn unknown_extra(name: &str, help: String) -> Self {
        Self::UnknownExtra {
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

    pub fn config_path(path: &std::path::Path) -> Self {
        Self::ConfigPath {
            path: path.display().to_string(),
        }
    }
}

impl From<ScaffoldError> for crate::error::BaudelaireErrorKind {
    fn from(e: ScaffoldError) -> Self {
        Self::Scaffold(Box::new(e))
    }
}
