//! Errors from deploying built files to a host.

use miette::Diagnostic;
use thiserror::Error;

/// A failure while deploying to a destination.
#[derive(Debug, Error, Diagnostic)]
pub enum DeployError {
    /// No `deploy { .. }` block, or it enables no backend.
    #[error("no deploy destination is configured")]
    #[diagnostic(
        code(baudelaire::deploy::unconfigured),
        help("add a `deploy {{ s3 {{ bucket \"..\" }} }}` block to config.kdl")
    )]
    Unconfigured,

    /// A required credential environment variable was unset or empty.
    #[error("missing credential: set `{var}`")]
    #[diagnostic(code(baudelaire::deploy::credentials))]
    MissingCredentials { var: String },

    /// The transport itself failed (DNS, TLS, connection, malformed response).
    #[error("request to the deploy host failed")]
    #[diagnostic(code(baudelaire::deploy::http))]
    Http {
        #[source]
        source: Box<ureq::Error>,
    },

    /// The host returned a non-2xx status; its own error body is carried through
    /// so the cause (auth, missing bucket, policy) is visible.
    #[error("{operation} `{key}` failed ({status}): {message}")]
    #[diagnostic(code(baudelaire::deploy::request))]
    Request {
        operation: &'static str,
        key: String,
        status: u16,
        message: String,
    },

    /// A bucket listing response could not be parsed.
    #[error("could not parse the bucket listing")]
    #[diagnostic(code(baudelaire::deploy::listing))]
    Listing { message: String },
}

impl From<ureq::Error> for DeployError {
    fn from(source: ureq::Error) -> Self {
        Self::Http {
            source: Box::new(source),
        }
    }
}
