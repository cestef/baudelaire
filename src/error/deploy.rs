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

    /// The SSH connection or transport failed (DNS, TCP, host key, protocol).
    #[error("ssh connection to `{host}` failed")]
    #[diagnostic(code(baudelaire::deploy::ssh::connect))]
    Connect {
        host: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The server rejected authentication for the user.
    #[error("ssh authentication as `{user}` failed")]
    #[diagnostic(
        code(baudelaire::deploy::ssh::auth),
        help("check the `key`/password and that the user is authorized on the host")
    )]
    Auth { user: String },

    /// An SFTP transfer or remote command failed.
    #[error("{operation} failed on the ssh host: {message}")]
    #[diagnostic(code(baudelaire::deploy::ssh::transfer))]
    Transfer {
        operation: &'static str,
        message: String,
    },
}

impl DeployError {
    /// A connection or transport failure to `host`, carrying its source.
    pub fn connect(
        host: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Connect {
            host: host.into(),
            source: Box::new(source),
        }
    }

    /// An SFTP or remote-command failure at `operation`.
    pub fn transfer(operation: &'static str, source: impl std::fmt::Display) -> Self {
        Self::Transfer {
            operation,
            message: source.to_string(),
        }
    }
}

impl From<ureq::Error> for DeployError {
    fn from(source: ureq::Error) -> Self {
        Self::Http {
            source: Box::new(source),
        }
    }
}
