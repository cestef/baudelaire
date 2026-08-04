//! Theme resolution failures.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Code, Text};

/// A configured theme that could not be resolved. Fatal: the site's templates
/// and assets are expected to come from it, so continuing would build a
/// stripped version of the site rather than the one that was asked for.
#[derive(Debug, Error, Diagnostic)]
pub enum ThemeError {
    #[error("{} is not a package spec: {}", Code(.spec), Text(.why))]
    #[diagnostic(
        code(baudelaire::theme::spec),
        help("a theme is named like any Typst package: `@preview/name:1.0.0`")
    )]
    Spec { spec: String, why: String },

    #[error("theme {} could not be obtained: {}", Code(.spec), Text(.why))]
    #[diagnostic(
        code(baudelaire::theme::unavailable),
        help("check the name and version, and that the machine can reach the package registry")
    )]
    Unavailable { spec: String, why: String },

    #[error("theme directory {} is outside the project", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::outside),
        help(
            "a Typst import cannot leave the project root: move the theme inside it, \
             or publish it and name it as `@namespace/name:version`"
        )
    )]
    Outside { path: String },

    #[error("theme directory {} does not exist", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::missing),
        help(
            "create it, `baudelaire theme add <name>` to write one of the shipped              themes there, or name a published theme as `@namespace/name:version`"
        )
    )]
    Missing { path: String },

    #[error("no theme named {} ships with baudelaire", Code(.name))]
    #[diagnostic(code(baudelaire::theme::unknown), help("{help}"))]
    Unknown { name: String, help: String },

    #[error("no theme baudelaire installed is at {}", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::uninstalled),
        help(
            "`baudelaire theme add <name>` writes one there; a theme you wrote or copied              in yourself is yours to move and delete"
        )
    )]
    Uninstalled { path: String },

    #[error("the theme record at {} could not be written: {}", Code(.path), Text(.why))]
    #[diagnostic(
        code(baudelaire::theme::lock),
        help("it records which files are baudelaire's, so `theme update` can keep yours")
    )]
    Lock { path: String, why: String },

    #[error("{} holds no files, so it is not a theme", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::empty),
        help(
            "a theme is `templates {{ }}`, `assets {{ }}`, `static {{ }}` and a `theme.kdl`,              in a directory of its own"
        )
    )]
    Empty { path: String },

    #[error("{} does not name a directory a theme could be called after", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::unnamed),
        help("a copy is known by its directory's name, so name the directory, or pass `--dir`")
    )]
    Unnamed { path: String },

    #[error("nothing knows how to fetch {}", Code(.spec))]
    #[diagnostic(
        code(baudelaire::theme::unsupported),
        help(
            "a theme comes from a name `baudelaire theme list` prints, or from a spelling              this build recognises; a copy whose record names a source this baudelaire              does not have was written by a newer one"
        )
    )]
    Unsupported { spec: String },
}

impl ThemeError {
    /// `help` is the nearest-name suggestion built from the bundled table, and
    /// arrives already marked up.
    pub fn unknown(name: &str, help: String) -> Self {
        Self::Unknown {
            name: name.to_owned(),
            help,
        }
    }

    /// A spec no source claims, or an origin no source owns: the same answer
    /// either way, because both mean this binary cannot go and get it.
    pub fn unsupported(spec: String) -> Self {
        Self::Unsupported { spec }
    }

    pub fn empty(path: &str) -> Self {
        Self::Empty {
            path: path.to_owned(),
        }
    }

    pub fn unnamed(path: &str) -> Self {
        Self::Unnamed {
            path: path.to_owned(),
        }
    }

    pub fn not_installed(path: &str) -> Self {
        Self::Uninstalled {
            path: path.to_owned(),
        }
    }

    /// The serializer's own message, kept as text.
    pub fn lock(path: impl std::fmt::Display, why: impl std::fmt::Display) -> Self {
        Self::Lock {
            path: path.to_string(),
            why: why.to_string(),
        }
    }

    /// The parser's own message, kept as text: naming typst's error type here
    /// would put the compiler's package API in this crate's error API for one
    /// string.
    pub fn spec(spec: &str, why: impl std::fmt::Display) -> Self {
        Self::Spec {
            spec: spec.to_owned(),
            why: why.to_string(),
        }
    }

    /// The package store's own message, kept as text for the same reason.
    pub fn unavailable(spec: &str, why: impl std::fmt::Display) -> Self {
        Self::Unavailable {
            spec: spec.to_owned(),
            why: why.to_string(),
        }
    }

    pub fn outside(path: &str) -> Self {
        Self::Outside {
            path: path.to_owned(),
        }
    }

    pub fn missing(path: &str) -> Self {
        Self::Missing {
            path: path.to_owned(),
        }
    }
}
