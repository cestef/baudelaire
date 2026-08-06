//! Errors from announcing to the AT Protocol (standard.site).

use std::fmt;

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Code, Text};

/// Which half of an announce a run was in when it stopped.
///
/// A typed label rather than a message the call site spells out, the same
/// discipline as [`super::deploy::Step`]: a new phase is a new variant, not a
/// new string.
#[derive(Debug, Clone, Copy)]
pub enum Stage {
    /// Writing the document records the site's pages call for.
    Send,
    /// Deleting the records no page backs any more.
    Remove,
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Send => "sending documents",
            Self::Remove => "removing stale records",
        })
    }
}

/// A failure while announcing standard.site records to a PDS.
#[derive(Debug, Error, Diagnostic)]
pub enum AnnounceError {
    /// No `announce { standard { .. } }` block, or it lacks a handle.
    #[error("standard.site announcing is not configured")]
    #[diagnostic(
        code(baudelaire::announce::unconfigured),
        help(
            "add a `announce {{ standard {{ handle \"you.example.com\" }} }}` block to config.kdl"
        )
    )]
    Unconfigured,

    /// The publication record requires a canonical base URL.
    #[error("standard.site announcing requires a base `url`")]
    #[diagnostic(
        code(baudelaire::announce::url),
        help("set `url \"https://example.com\"` in config.kdl or pass `--base-url`")
    )]
    NoUrl,

    /// Authentication (`createSession`) succeeded at the HTTP layer but the
    /// response was missing or rejected.
    ///
    /// `message` is written at the call site, not read off the response, so it
    /// is already marked up (see [`crate::ui::markup!`]) and interpolated as-is
    /// rather than escaped.
    #[error("atproto authentication failed: {message}")]
    #[diagnostic(
        code(baudelaire::announce::auth),
        help("check the handle and app password")
    )]
    Auth { message: String },

    /// An XRPC method returned a non-2xx status. The PDS's own error body is
    /// carried through so the cause (rate limit, bad record, invalid session)
    /// is visible.
    #[error("XRPC {} failed ({status}): {}", Code(.nsid), Text(.message))]
    #[diagnostic(code(baudelaire::announce::xrpc))]
    Xrpc {
        nsid: String,
        status: u16,
        message: String,
    },

    /// The transport itself failed (DNS, TLS, connection, malformed response).
    #[error("HTTP request to the PDS failed")]
    #[diagnostic(code(baudelaire::announce::http))]
    Http {
        #[source]
        source: Box<ureq::Error>,
    },

    /// The configured `did` does not match the account that authenticated: the
    /// build would have emitted verification artifacts for the wrong identity.
    #[error(
        "configured did {} does not match the authenticated account {}",
        Code(.configured),
        Code(.actual)
    )]
    #[diagnostic(
        code(baudelaire::announce::did),
        // The nested block, not `announce.standard.did`: the parser refuses a
        // dotted key with `unknown config key`, so this named a spelling
        // config.kdl has never accepted. Braces doubled, the literal being a
        // format string first.
        help("update `announce {{ standard {{ did }} }}` to the authenticated did, or remove it")
    )]
    DidMismatch { configured: String, actual: String },

    /// A run that stopped partway through, naming where it stopped.
    ///
    /// `putRecord` is an upsert and `deleteRecord` is idempotent, so a partial
    /// run corrupts nothing; what it costs is the work already done, which a
    /// re-run repeats from zero. The skip-cache is written before this is
    /// raised, so the re-run skips what was sent, and the position is named
    /// here so the operator can see how far it got instead of guessing.
    #[error("{stage} stopped after {done} of {total}, at {}", Code(.at))]
    #[diagnostic(
        code(baudelaire::announce::interrupted),
        help(
            "the records already written are recorded, so re-running resumes rather than \
             starting over; nothing is duplicated either way"
        )
    )]
    Interrupted {
        stage: Stage,
        done: usize,
        total: usize,
        at: String,
        #[source]
        source: Box<Self>,
    },

    /// A paginated read kept returning a cursor past a sane page ceiling: a
    /// misbehaving or hostile PDS can advance the cursor forever, so the walk is
    /// bounded and refused rather than looping (or growing memory) unbounded.
    #[error("XRPC {} did not stop paginating after {pages} pages", Code(.nsid))]
    #[diagnostic(
        code(baudelaire::announce::pagination),
        help(
            "the PDS keeps returning a pagination cursor; check that `pds` points at a real server"
        )
    )]
    Pagination { nsid: String, pages: usize },
}

impl AnnounceError {
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

    /// A run that stopped at `at`, having done `done` of `total`, keeping the
    /// failure that stopped it.
    pub fn interrupted(
        stage: Stage,
        done: usize,
        total: usize,
        at: impl Into<String>,
        source: Self,
    ) -> Self {
        Self::Interrupted {
            stage,
            done,
            total,
            at: at.into(),
            source: Box::new(source),
        }
    }
}

impl From<ureq::Error> for AnnounceError {
    fn from(source: ureq::Error) -> Self {
        Self::Http {
            source: Box::new(source),
        }
    }
}
