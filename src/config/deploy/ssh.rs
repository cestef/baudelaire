//! `deploy { ssh { } }`: a host reachable over SSH.

use std::path::PathBuf;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// A host reachable over SSH. Files are reconciled with the remote directory
/// over SFTP, and change detection runs `sha256sum` on the host so an unchanged
/// file is never re-sent.
#[derive(Debug, Clone, Hash, Table)]
pub struct SshConfig {
    /// The host uploaded to.
    #[key(text)]
    pub host: String,

    /// The remote directory the site is written into.
    ///
    /// Absolute.
    #[key(text)]
    pub path: String,

    /// The SSH port.
    #[key(port)]
    pub port: u16,

    /// The user to connect as.
    ///
    /// Defaults to `$USER`.
    #[key(opt text)]
    pub user: Option<String>,

    /// The private key to authenticate with. Prefer an ed25519 key.
    ///
    /// Absolute, `~`-relative, or under the project root. When unset,
    /// authentication tries the ssh-agent, then a password from the
    /// environment or prompt.
    #[key(opt path)]
    pub key: Option<PathBuf>,

    /// Verify the host key against `known_hosts`, learning an unseen host on first connect and refusing a changed one.
    ///
    /// Off accepts any key.
    #[key(flag)]
    pub strict: bool,

    /// Delete remote files this build did not produce.
    #[key(flag)]
    pub delete: bool,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            path: String::new(),
            port: 22,
            user: None,
            key: None,
            strict: true,
            delete: true,
        }
    }
}
