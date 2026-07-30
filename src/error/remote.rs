//! Errors shared by the destinations baudelaire pushes to: the announce and
//! deploy layers both resolve credentials and prompt through [`crate::remote`].

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Text;

/// A failure common to any remote destination.
#[derive(Debug, Error, Diagnostic)]
pub enum RemoteError {
    /// A secret was not supplied and could not be prompted for.
    #[error("no {label} supplied")]
    #[diagnostic(
        code(baudelaire::remote::secret),
        help(
            "pass it on the command line, set its environment variable, or run in a terminal to be prompted"
        )
    )]
    MissingSecret { label: String },

    /// A mutating action needed confirming, with no terminal to confirm at.
    ///
    /// Reached only once the action is known to need an answer: `--yes` and
    /// `--dry-run` are both settled before anything asks. Taking the prompt's
    /// default here instead is what let a CI run that forgot `--yes` skip every
    /// backend and exit 0 having published nothing.
    #[error("cannot confirm {} without a terminal", Text(.action))]
    #[diagnostic(
        code(baudelaire::remote::needs_confirmation),
        help("pass `--yes` to confirm non-interactively, or `--dry-run` to preview")
    )]
    NeedsConfirmation { action: String },
}
