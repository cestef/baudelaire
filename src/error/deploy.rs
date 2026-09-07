//! Errors from deploying built files to a host, where what was being done is a
//! typed label rather than a message the call site spells out.

use std::fmt;

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Code, Text};

/// An HTTP method the S3 client signs and sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Put,
    Delete,
}

impl Method {
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

/// A step of an ssh deploy that runs on the host.
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

/// Which half of a reconcile a run was in when it stopped.
#[derive(Debug, Clone, Copy)]
pub enum Phase {
    Upload,
    Delete,
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Upload => "uploading",
            Self::Delete => "deleting",
        })
    }
}

/// A `deploy { }` setting a destination cannot work without.
#[derive(Debug, Clone, Copy)]
pub enum Required {
    SshHost,
    SshPath,
    S3Bucket,
}

impl Required {
    /// The setting as `config.kdl` spells it, and what belongs in it.
    const fn spellings(self) -> (&'static str, &'static str) {
        match self {
            Self::SshHost => (
                "deploy { ssh { host } }",
                "name the server, as `ssh` would take it: a hostname or an IP",
            ),
            Self::SshPath => (
                "deploy { ssh { path } }",
                "give the absolute path of the directory the site is written into, such as `/var/www/site`",
            ),
            Self::S3Bucket => ("deploy { s3 { bucket } }", "name the bucket to upload into"),
        }
    }

    const fn help(self) -> &'static str {
        self.spellings().1
    }
}

impl fmt::Display for Required {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spellings().0)
    }
}

#[derive(Debug, Error, Diagnostic)]
pub enum DeployError {
    #[error("no deploy destination is configured")]
    #[diagnostic(
        code(baudelaire::deploy::unconfigured),
        help("add a `deploy {{ s3 {{ bucket \"..\" }} }}` block to config.kdl")
    )]
    Unconfigured,

    #[error("this build has no SSH deploy backend")]
    #[diagnostic(
        code(baudelaire::deploy::ssh_unsupported),
        help("rebuild with the `ssh` cargo feature, or deploy over `s3 {{ }}`")
    )]
    #[cfg(not(feature = "ssh"))]
    SshUnsupported,

    #[error("missing credential: set {}", Code(.var))]
    #[diagnostic(code(baudelaire::deploy::credentials))]
    MissingCredentials { var: String },

    /// Refused before anything connects: every empty spelling means something,
    /// and an empty `path` would make the deploy root `/`.
    #[error("{} is required and was left empty", Code(.setting))]
    #[diagnostic(code(baudelaire::deploy::required), help("{}", setting.help()))]
    Required { setting: Required },

    /// Both are spliced into the request authority, so anything but a plain
    /// name can send the signed credential to a host the author did not write.
    #[error("{} is not a name: {}", Code(.setting), Code(.got))]
    #[diagnostic(
        code(baudelaire::deploy::not_a_name),
        help(
            "this is spliced into the host the signed request goes to, so it may not carry `/`, `?`, `#`, `@`, `:` or whitespace"
        )
    )]
    NotAName { setting: &'static str, got: String },

    /// The remote path is joined by string and never resolved, so a relative
    /// one lands wherever the host starts the SFTP session.
    #[error("{} is not an absolute remote path", Code(.path))]
    #[diagnostic(
        code(baudelaire::deploy::relative),
        help(
            "write it from the root, such as `/var/www/site`: a relative path is resolved by \
             the host against wherever the session starts, which is not something this can know"
        )
    )]
    Relative { path: String },

    /// Uploads and deletes are individually idempotent, so a half-finished run
    /// corrupts nothing and re-running finishes it.
    #[error("{phase} stopped after {done} of {total}, at {}", Code(.key))]
    #[diagnostic(
        code(baudelaire::deploy::interrupted),
        help(
            "the remote holds everything up to this point; re-running compares digests again \
             and sends only what still differs"
        )
    )]
    Interrupted {
        phase: Phase,
        done: usize,
        total: usize,
        key: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("request to the deploy host failed")]
    #[diagnostic(code(baudelaire::deploy::http))]
    Http {
        #[source]
        source: Box<ureq::Error>,
    },

    #[error("{method} {} failed ({status}): {}", Code(.uri), Text(.message))]
    #[diagnostic(code(baudelaire::deploy::request), help("{}", Self::hint(*status)))]
    Request {
        method: Method,
        uri: String,
        status: u16,
        message: String,
    },

    /// A walk that never reaches the end must fail rather than return a short
    /// list, which here would delete remote files the listing never mentioned.
    #[error("the bucket listing did not end after {pages} pages")]
    #[diagnostic(
        code(baudelaire::deploy::pagination),
        help(
            "the host keeps returning a continuation token; check that `endpoint` points at a real S3 service"
        )
    )]
    Pagination { pages: usize },

    #[error("could not parse the bucket listing")]
    #[diagnostic(code(baudelaire::deploy::listing))]
    Listing {
        #[source]
        source: roxmltree::Error,
    },

    #[error("ssh connection to {} failed", Code(.host))]
    #[diagnostic(code(baudelaire::deploy::ssh::connect))]
    Connect {
        host: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The check is made against one host *and port*, so the remedy has to name
    /// the same pair: see [`DeployError::entry`].
    #[error("the host key for {} has changed", Code(.host))]
    #[diagnostic(
        code(baudelaire::deploy::ssh::host_key),
        help(
            "if you trust the change, run `ssh-keygen -R {}`; else set `strict #false`",
            Text(Self::entry(host, *port))
        )
    )]
    HostKeyChanged { host: String, port: u16 },

    /// Distinct from a changed key: nothing was compared at all, which under
    /// `strict` is a refusal rather than the connection failing for its own
    /// reasons.
    #[error("the host key for {} could not be checked", Code(.host))]
    #[diagnostic(
        code(baudelaire::deploy::ssh::host_key_unverifiable),
        help("make `~/.ssh/known_hosts` readable, or set `strict #false` to connect without it")
    )]
    HostKeyUnverifiable { host: String },

    #[error("no ssh user configured and `$USER` is unset")]
    #[diagnostic(
        code(baudelaire::deploy::ssh::no_user),
        help("set `deploy {{ ssh {{ user \"…\" }} }}`")
    )]
    NoUser,

    #[error("ssh authentication as {} failed", Code(.user))]
    #[diagnostic(
        code(baudelaire::deploy::ssh::auth),
        help("check the `key`/password and that the user is authorized on the host")
    )]
    Auth { user: String },

    /// Its own error rather than a decode failure: a key that was never opened
    /// must not draw a passphrase prompt.
    #[error("no private key at {}", Code(.path))]
    #[diagnostic(
        code(baudelaire::deploy::ssh::key_missing),
        help(
            "check `deploy {{ ssh {{ key }} }}`: a leading `~` is expanded against `$HOME`, \
             and nothing else in the path is"
        )
    )]
    KeyMissing { path: String },

    #[error("the private key at {} could not be read", Code(.path))]
    #[diagnostic(
        code(baudelaire::deploy::ssh::key_unreadable),
        help(
            "check the file's ownership and mode; a private key is usually `0600` and owned \
             by the user this runs as"
        )
    )]
    KeyUnreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{step} failed on the ssh host")]
    #[diagnostic(code(baudelaire::deploy::ssh::transfer))]
    Transfer {
        step: Step,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Split from [`DeployError::Transfer`], whose message names the host, so a
    /// local failure does not send the reader to the wrong end of the wire.
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
    pub fn connect(
        host: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Connect {
            host: host.into(),
            source: Box::new(source),
        }
    }

    pub fn transfer(step: Step, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Transfer {
            step,
            source: Box::new(source),
        }
    }

    pub fn local(step: Setup, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Local {
            step,
            source: Box::new(source),
        }
    }

    pub fn interrupted(
        phase: Phase,
        done: usize,
        total: usize,
        key: &str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Interrupted {
            phase,
            done,
            total,
            key: key.to_owned(),
            source: Box::new(source),
        }
    }

    pub fn required(setting: Required) -> Self {
        Self::Required { setting }
    }

    pub fn not_a_name(setting: &'static str, got: &str) -> Self {
        Self::NotAName {
            setting,
            got: got.to_owned(),
        }
    }

    pub fn request(method: Method, uri: &str, status: u16, body: &str) -> Self {
        /// A 403's XML blob or a proxy's whole HTML error page is not worth a
        /// screenful.
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

    /// What a status most often means here, so auth failure, a missing bucket
    /// and a rate limit are told apart without reading XML.
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

    pub fn host_key_changed(host: impl Into<String>, port: u16) -> Self {
        Self::HostKeyChanged {
            host: host.into(),
            port,
        }
    }

    pub fn host_key_unverifiable(host: impl Into<String>) -> Self {
        Self::HostKeyUnverifiable { host: host.into() }
    }

    /// The port a `known_hosts` line is written without brackets for.
    const PORT: u16 = 22;

    /// The `known_hosts` entry the check was made against, which is what
    /// `ssh-keygen -R` has to be handed.
    ///
    /// Any other port is recorded as `[host]:port` and quoted, since `[` and
    /// `]` are glob characters the shell would expand.
    pub(crate) fn entry(host: &str, port: u16) -> String {
        match port {
            Self::PORT => host.to_owned(),
            port => format!("'[{host}]:{port}'"),
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

impl From<roxmltree::Error> for DeployError {
    fn from(source: roxmltree::Error) -> Self {
        Self::Listing { source }
    }
}

#[cfg(test)]
mod tests {
    use super::DeployError;

    #[test]
    fn the_host_key_remedy_names_the_entry_the_check_used() {
        assert_eq!(DeployError::entry("srv.example", 22), "srv.example");
        assert_eq!(
            DeployError::entry("srv.example", 2222),
            "'[srv.example]:2222'"
        );
    }

    #[test]
    fn the_host_key_help_carries_the_port() {
        let help = miette::Diagnostic::help(&DeployError::host_key_changed("srv.example", 2222))
            .expect("help")
            .to_string();
        assert!(help.contains("[srv.example]:2222"), "{help}");
    }
}
