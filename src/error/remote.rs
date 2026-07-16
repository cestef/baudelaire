//! Errors shared by the destinations baudelaire pushes to — the announce and
//! deploy layers both resolve credentials and prompt through [`crate::remote`].

use miette::Diagnostic;
use thiserror::Error;

/// A failure common to any remote destination.
#[derive(Debug, Error, Diagnostic)]
pub enum RemoteError {
    /// A secret was not supplied and could not be prompted for.
    #[error("no {label} supplied")]
    #[diagnostic(
        code(baudelaire::remote::secret),
        help("pass it on the command line, set its environment variable, or run in a terminal to be prompted")
    )]
    MissingSecret { label: String },
}
