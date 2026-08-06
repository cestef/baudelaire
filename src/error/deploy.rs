//! Errors from deploying built files to a host.
//!
//! Same discipline as [`super::fs`]: what was being done is a typed label
//! ([`Method`], [`Step`], [`Setup`], [`Phase`], [`Required`]) rather than a
//! message the call site spells out, so every message reads the same way and a
//! new operation is a new variant, not a new string.

use std::fmt;

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Code, Text};

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

/// Which half of a reconcile a run was in when it stopped. Shared by every
/// destination, because [`Dist::reconcile`](crate::deploy::Dist::reconcile) is.
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
///
/// A typed label rather than a message at the call site, the same discipline as
/// [`Step`] and [`Setup`]: the key's spelling and the sentence that says what to
/// write in it live together, so a destination that gains a required setting
/// gains one variant here and cannot describe it two ways.
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

    /// What to write in it.
    const fn help(self) -> &'static str {
        self.spellings().1
    }
}

impl fmt::Display for Required {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spellings().0)
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
    #[error("missing credential: set {}", Code(.var))]
    #[diagnostic(code(baudelaire::deploy::credentials))]
    MissingCredentials { var: String },

    /// A destination block is present but a setting it cannot work without was
    /// left empty.
    ///
    /// Refused before anything connects, because the empty spellings all mean
    /// something and none of them means what was written: an empty `path` made
    /// the deploy root `/`, so `index.html` became `/index.html` and the run
    /// issued `create_dir("/assets")` on the host. `deploy { ssh { host "srv" } }`
    /// with no `path` at all parsed, and did exactly that.
    #[error("{} is required and was left empty", Code(.setting))]
    #[diagnostic(code(baudelaire::deploy::required), help("{}", setting.help()))]
    Required { setting: Required },

    /// `deploy { ssh { path } }` naming a path that does not start at the root.
    ///
    /// The remote path is joined by string, not resolved, so a relative one is
    /// resolved by the *host* against whatever directory the SFTP session
    /// happens to start in, which is the login user's home on OpenSSH and
    /// nothing in particular anywhere else. A deploy that cannot say where it
    /// wrote is not a deploy.
    #[error("{} is not an absolute remote path", Code(.path))]
    #[diagnostic(
        code(baudelaire::deploy::relative),
        help(
            "write it from the root, such as `/var/www/site`: a relative path is resolved by \
             the host against wherever the session starts, which is not something this can know"
        )
    )]
    Relative { path: String },

    /// A reconcile that stopped partway through, naming where it stopped.
    ///
    /// Uploads and deletes are individually idempotent, so a half-finished run
    /// leaves the remote consistent with neither the old site nor the new one
    /// but corrupts nothing: re-running finishes it, and sends only what still
    /// differs. What it used to leave out was any word about how much of the
    /// site had already changed under the reader's feet.
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
    #[error("{method} {} failed ({status}): {}", Code(.uri), Text(.message))]
    #[diagnostic(code(baudelaire::deploy::request), help("{}", Self::hint(*status)))]
    Request {
        method: Method,
        uri: String,
        status: u16,
        message: String,
    },

    /// The bucket kept handing back a continuation token past the page ceiling.
    /// The same shape and the same reasoning as `atproto`'s: a walk that never
    /// reaches the end must fail loudly rather than return a short list, which
    /// here would mean deleting remote files the listing never mentioned.
    #[error("the bucket listing did not end after {pages} pages")]
    #[diagnostic(
        code(baudelaire::deploy::pagination),
        help(
            "the host keeps returning a continuation token; check that `endpoint` points at a real S3 service"
        )
    )]
    Pagination { pages: usize },

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
    #[error("ssh connection to {} failed", Code(.host))]
    #[diagnostic(code(baudelaire::deploy::ssh::connect))]
    Connect {
        host: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The server presented a different host key than the one recorded in
    /// `known_hosts`, the man-in-the-middle guard.
    ///
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

    /// No SSH user could be resolved.
    #[error("no ssh user configured and `$USER` is unset")]
    #[diagnostic(
        code(baudelaire::deploy::ssh::no_user),
        help("set `deploy {{ ssh {{ user \"…\" }} }}`")
    )]
    NoUser,

    /// The server rejected authentication for the user.
    #[error("ssh authentication as {} failed", Code(.user))]
    #[diagnostic(
        code(baudelaire::deploy::ssh::auth),
        help("check the `key`/password and that the user is authorized on the host")
    )]
    Auth { user: String },

    /// `deploy { ssh { key } }` names a file that is not there.
    ///
    /// Its own error rather than one of the two below it, because a key that
    /// was never opened is not a key that failed to decode. Every failure to
    /// read the file used to answer with a passphrase prompt, so a typo in the
    /// path asked for the passphrase of a file nobody had read, and then
    /// blamed whatever was typed for not decrypting it.
    #[error("no private key at {}", Code(.path))]
    #[diagnostic(
        code(baudelaire::deploy::ssh::key_missing),
        help(
            "check `deploy {{ ssh {{ key }} }}`: a leading `~` is expanded against `$HOME`, \
             and nothing else in the path is"
        )
    )]
    KeyMissing { path: String },

    /// The private key is there and could not be read: a mode that excludes
    /// this user, a directory in the way, a file that is not text.
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

    /// A reconcile that stopped at `key`, having done `done` of `total`,
    /// keeping the failure that stopped it.
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

    /// A destination whose block leaves `setting` empty.
    pub fn required(setting: Required) -> Self {
        Self::Required { setting }
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

    /// The host key changed for `host` on `port`.
    pub fn host_key_changed(host: impl Into<String>, port: u16) -> Self {
        Self::HostKeyChanged {
            host: host.into(),
            port,
        }
    }

    /// The port a `known_hosts` line is written without brackets for.
    ///
    /// SSH's own default, not this config's: the bracketed form exists because
    /// the file has to distinguish two servers at one address, and the protocol
    /// port is the one that needs no distinguishing. `deploy { ssh { port } }`
    /// defaults to the same number for a different reason.
    const PORT: u16 = 22;

    /// The `known_hosts` entry the check was made against, which is what
    /// `ssh-keygen -R` has to be handed.
    ///
    /// OpenSSH records a non-default port as `[host]:port` and so does the
    /// check here, so `ssh-keygen -R host` matches no line and removes nothing:
    /// the help used to name a command that exits 0 and leaves the changed key
    /// exactly where it was. Quoted at the other port, because `[` and `]` are
    /// glob characters and an unquoted entry is a pattern the shell expands.
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

    /// The remedy has to name the entry the check was made against. A
    /// non-default port is recorded as `[host]:port`, so the bare-host command
    /// this used to print matched nothing and removed nothing, while reporting
    /// success.
    #[test]
    fn the_host_key_remedy_names_the_entry_the_check_used() {
        assert_eq!(DeployError::entry("srv.example", 22), "srv.example");
        assert_eq!(
            DeployError::entry("srv.example", 2222),
            "'[srv.example]:2222'"
        );
    }

    /// ...and the help actually carries it, since that is the whole point of
    /// keeping the port on the variant.
    #[test]
    fn the_host_key_help_carries_the_port() {
        let help = miette::Diagnostic::help(&DeployError::host_key_changed("srv.example", 2222))
            .expect("help")
            .to_string();
        assert!(help.contains("[srv.example]:2222"), "{help}");
    }
}
