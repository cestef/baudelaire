//! `deploy { ssh { } }`: a host reachable over SSH.

use std::path::PathBuf;

use crate::config::dispatch::Kind::{Flag, Number, Path, Text};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// A host reachable over SSH. Files are reconciled with the remote directory
/// over SFTP; change detection runs `sha256sum` on the host so an unchanged file
/// is never re-sent. Works against any OpenSSH-compatible server.
#[derive(Debug, Clone, Hash)]
pub struct SshConfig {
    /// Hostname or IP of the server.
    pub host: String,
    /// Absolute path to the remote directory the build is mirrored into.
    pub path: String,
    /// Port the SSH server listens on.
    pub port: u16,
    /// User to authenticate as. Defaults to `$USER`.
    pub user: Option<String>,
    /// Path to a private key (absolute, `~`-relative, or under the project
    /// root). When unset, authentication tries the ssh-agent, then a password
    /// from the environment/prompt.
    pub key: Option<PathBuf>,
    /// Verify the server's host key against `~/.ssh/known_hosts`, learning an
    /// unseen host on first connect and refusing a changed key (MITM guard).
    /// Turn off to accept any key (`StrictHostKeyChecking=no`).
    pub strict: bool,
    /// Delete remote files under `path` that the build no longer produces.
    pub delete: bool,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            path: String::new(),
            // The standard SSH port.
            port: 22,
            // None resolves to $USER at deploy time.
            user: None,
            // None falls back to agent, then password auth.
            key: None,
            // secure by default: verify host keys against known_hosts.
            strict: true,
            // reconcile: remove what the build no longer produces.
            delete: true,
        }
    }
}

/// The `ssh { .. }` block: presence enables the SSH backend.
impl Section for SshConfig {
    const RULES: Block<Self> = Block(&[
        ("host", Text, "The host uploaded to.", |c, n, t| {
            c.host = n.string(t, 0)?;
            Ok(())
        }),
        (
            "path",
            Text,
            "The remote directory the site is written into.",
            |c, n, t| {
                c.path = n.string(t, 0)?;
                Ok(())
            },
        ),
        ("port", Number, "The SSH port.", |c, n, t| {
            c.port = n.port(t, 0)?;
            Ok(())
        }),
        ("user", Text, "The user to connect as.", |c, n, t| {
            c.user = Some(n.string(t, 0)?);
            Ok(())
        }),
        (
            "key",
            Path,
            "The private key to authenticate with. Prefer an ed25519 key.",
            |c, n, t| {
                c.key = Some(n.string(t, 0)?.into());
                Ok(())
            },
        ),
        (
            "strict",
            Flag,
            "Refuse to connect to a host whose key is not already known.",
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "delete",
            Flag,
            "Delete remote files this build did not produce.",
            |c, n, t| {
                c.delete = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
