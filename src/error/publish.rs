//! Errors from publishing to the AT Protocol (standard.site).

use miette::Diagnostic;
use thiserror::Error;

/// A failure while publishing standard.site records to a PDS.
#[derive(Debug, Error, Diagnostic)]
pub enum PublishError {
    /// No `publish { standard { .. } }` block, or it lacks a handle.
    #[error("standard.site publishing is not configured")]
    #[diagnostic(
        code(baudelaire::publish::unconfigured),
        help(
            "add a `publish {{ standard {{ handle \"you.example.com\" }} }}` block to config.kdl"
        )
    )]
    Unconfigured,

    /// The publication record requires a canonical base URL.
    #[error("standard.site publishing requires a base `url`")]
    #[diagnostic(
        code(baudelaire::publish::url),
        help("set `url \"https://example.com\"` in config.kdl or pass `--base-url`")
    )]
    NoUrl,

    /// A destination's secret was not supplied and could not be prompted for.
    #[error("no {label} supplied for publishing")]
    #[diagnostic(
        code(baudelaire::publish::secret),
        help(
            "pass `--password`, set its environment variable, or run in a terminal to be prompted"
        )
    )]
    MissingSecret { label: String },

    /// Authentication (`createSession`) succeeded at the HTTP layer but the
    /// response was missing or rejected.
    #[error("atproto authentication failed: {message}")]
    #[diagnostic(
        code(baudelaire::publish::auth),
        help("check the handle and app password")
    )]
    Auth { message: String },

    /// An XRPC method returned a non-2xx status. The PDS's own error body is
    /// carried through so the cause (rate limit, bad record, invalid session)
    /// is visible.
    #[error("XRPC `{nsid}` failed ({status}): {message}")]
    #[diagnostic(code(baudelaire::publish::xrpc))]
    Xrpc {
        nsid: String,
        status: u16,
        message: String,
    },

    /// The transport itself failed (DNS, TLS, connection, malformed response).
    #[error("HTTP request to the PDS failed")]
    #[diagnostic(code(baudelaire::publish::http))]
    Http {
        #[source]
        source: Box<ureq::Error>,
    },

    /// The configured `did` does not match the account that authenticated — the
    /// build would have emitted verification artifacts for the wrong identity.
    #[error("configured did `{configured}` does not match the authenticated account `{actual}`")]
    #[diagnostic(
        code(baudelaire::publish::did),
        help("update `publish.standard.did` to the authenticated did, or remove it")
    )]
    DidMismatch { configured: String, actual: String },

    /// The configured publication icon could not be read.
    #[error("could not read publication icon `{path}`")]
    #[diagnostic(code(baudelaire::publish::icon))]
    Icon {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl PublishError {
    pub fn auth(message: impl Into<String>) -> Self {
        Self::Auth {
            message: message.into(),
        }
    }

    pub fn xrpc(nsid: impl Into<String>, status: u16, message: impl Into<String>) -> Self {
        Self::Xrpc {
            nsid: nsid.into(),
            status,
            message: message.into(),
        }
    }
}

impl From<ureq::Error> for PublishError {
    fn from(source: ureq::Error) -> Self {
        Self::Http {
            source: Box::new(source),
        }
    }
}
