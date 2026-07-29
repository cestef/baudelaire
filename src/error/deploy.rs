//! Errors from deploying built files to a host.
//!
//! Same discipline as [`super::fs`]: what was being done is a typed label
//! ([`Method`], [`Step`], [`Setup`]) rather than a message the call site spells
//! out, so every message reads the same way and a new operation is a new
//! variant, not a new string.

use std::fmt;

use miette::Diagnostic;
use thiserror::Error;

/// An HTTP method the S3 client signs and sends. The whole set it uses: a
/// listing, an upload, a removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Put,
    Delete,
}

impl Method {
    /// The wire spelling, which is also what a canonical request signs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A step of an ssh deploy that runs on the host, named for error messages.
#[derive(Debug, Clone, Copy)]
pub enum Step {
    Authenticate,
    OpenSftp,
    Exec,
    Upload,
    Delete,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Authenticate => "authenticate",
            Self::OpenSftp => "open sftp",
            Self::Exec => "exec",
            Self::Upload => "upload",
            Self::Delete => "delete",
        })
    }
}

/// A step of an ssh deploy that runs on this machine, before anything reaches
/// the host.
#[derive(Debug, Clone, Copy)]
pub enum Setup {
    Runtime,
    PrivateKey,
}

impl fmt::Display for Setup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Runtime => "starting the async runtime",
            Self::PrivateKey => "loading the private key",
        })
    }
}

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

    /// `deploy { ssh }` is the only destination, and this binary was built
    /// without the `ssh` feature that compiles the backend in.
    #[error("this build has no SSH deploy backend")]
    #[diagnostic(
        code(baudelaire::deploy::ssh_unsupported),
        help("rebuild with the `ssh` cargo feature, or deploy over `s3 {{ }}`")
    )]
    #[cfg(not(feature = "ssh"))]
    SshUnsupported,

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

    /// The host returned a non-2xx status. Its own error body is carried
    /// through (truncated) so the cause is visible, with a status-keyed hint,
    /// because the body alone leaves auth failure, a missing bucket and a rate
    /// limit indistinguishable.
    #[error("{method} `{uri}` failed ({status}): {message}")]
    #[diagnostic(code(baudelaire::deploy::request), help("{}", Self::hint(*status)))]
    Request {
        method: Method,
        uri: String,
        status: u16,
        message: String,
    },

    /// A bucket listing response was not the XML a `ListObjectsV2` answer must
    /// be. The parser's own error is kept as the source: it names the offending
    /// position, which a flattened message would drop.
    #[error("could not parse the bucket listing")]
    #[diagnostic(code(baudelaire::deploy::listing))]
    Listing {
        #[source]
        source: roxmltree::Error,
    },

    /// The SSH connection or transport failed (DNS, TCP, host key, protocol).
    #[error("ssh connection to `{host}` failed")]
    #[diagnostic(code(baudelaire::deploy::ssh::connect))]
    Connect {
        host: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The server presented a different host key than the one recorded in
    /// `known_hosts`, the man-in-the-middle guard.
    #[error("the host key for `{host}` has changed")]
    #[diagnostic(
        code(baudelaire::deploy::ssh::host_key),
        help("if you trust the change, run `ssh-keygen -R {host}`; else set `strict #false`")
    )]
    HostKeyChanged { host: String },

    /// No SSH user could be resolved.
    #[error("no ssh user configured and `$USER` is unset")]
    #[diagnostic(
        code(baudelaire::deploy::ssh::no_user),
        help("set `deploy {{ ssh {{ user \"…\" }} }}`")
    )]
    NoUser,

    /// The server rejected authentication for the user.
    #[error("ssh authentication as `{user}` failed")]
    #[diagnostic(
        code(baudelaire::deploy::ssh::auth),
        help("check the `key`/password and that the user is authorized on the host")
    )]
    Auth { user: String },

    /// An SFTP transfer or remote command failed.
    #[error("{step} failed on the ssh host")]
    #[diagnostic(code(baudelaire::deploy::ssh::transfer))]
    Transfer {
        step: Step,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A local step of an ssh deploy failed, before anything reached the host:
    /// building the async runtime, decoding a private key.
    ///
    /// Split from [`DeployError::Transfer`] because that variant's message
    /// names the host, and reporting "start runtime failed on the ssh host" for
    /// a Tokio runtime this machine could not build sends the reader to debug
    /// the wrong end of the connection.
    #[error("{step} failed")]
    #[diagnostic(
        code(baudelaire::deploy::ssh::local),
        help("this failed locally, before contacting the host")
    )]
    Local {
        step: Setup,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
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

    /// An SFTP or remote-command failure at `step`, keeping its source.
    pub fn transfer(step: Step, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Transfer {
            step,
            source: Box::new(source),
        }
    }

    /// A failure at `step` on this machine, before the host is involved.
    pub fn local(step: Setup, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Local {
            step,
            source: Box::new(source),
        }
    }

    /// A non-2xx response, with the host's body clamped and a hint chosen from
    /// the status.
    pub fn request(method: Method, uri: &str, status: u16, body: &str) -> Self {
        /// A 403's XML blob, or a proxy's whole HTML error page, is not worth a
        /// screenful; the first line or two carries the code.
        const LIMIT: usize = 400;
        let body = body.trim();
        let message = match body.char_indices().nth(LIMIT) {
            Some((cut, _)) => format!("{}…", &body[..cut]),
            None => body.to_owned(),
        };
        Self::Request {
            method,
            uri: uri.to_owned(),
            status,
            message,
        }
    }

    /// What a status most often means here, so the three common causes are told
    /// apart without reading XML. Derived on demand rather than stored, so it
    /// cannot contradict the status it explains.
    fn hint(status: u16) -> &'static str {
        match status {
            401 | 403 => {
                "check the access key, secret, and (for temporary credentials) `AWS_SESSION_TOKEN`, and that the key may write this bucket"
            }
            404 => "check the `bucket` name and `region`, and that the bucket exists",
            429 | 503 => "the host is rate limiting; retry",
            500..=599 => "the host or a proxy in front of it failed; retry",
            _ => "check the `deploy { s3 }` block against the host's requirements",
        }
    }

    /// The host key changed for `host`.
    pub fn host_key_changed(host: impl Into<String>) -> Self {
        Self::HostKeyChanged { host: host.into() }
    }
}

impl From<ureq::Error> for DeployError {
    fn from(source: ureq::Error) -> Self {
        Self::Http {
            source: Box::new(source),
        }
    }
}

impl From<roxmltree::Error> for DeployError {
    fn from(source: roxmltree::Error) -> Self {
        Self::Listing { source }
    }
}
