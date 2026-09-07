//! Host-key verification, and the `russh` client handler that applies it:
//! strict mirrors OpenSSH, non-strict accepts any key; either way the check
//! records what it concluded, so nothing is trusted in silence.

use std::sync::Arc;

use parking_lot::Mutex;

use russh::client;
use russh::keys::known_hosts;
use russh::keys::{Error as KeyError, HashAlg, PublicKey};

use crate::config::SshConfig;

/// The verdict of checking a server key against `known_hosts`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Recorded and matching.
    Trusted,
    /// Not recorded before, and now written: trust on first use.
    Learned,
    /// Recorded but different: refused under `strict`, warned about without it.
    Changed,
    /// The file could not be read or parsed.
    Unverifiable,
}

impl Verdict {
    /// Whether the connection may go ahead. Without `strict` every verdict is
    /// accepted; the caller is the one that says so out loud.
    pub const fn accepts(self, strict: bool) -> bool {
        !strict || matches!(self, Self::Trusted | Self::Learned)
    }
}

/// What checking the server's key concluded, and the key it concluded about.
pub struct Checked {
    pub verdict: Verdict,
    /// The `SHA256:…` form an operator compares out of band, which is only
    /// worth reporting for a verdict that is not a plain match.
    pub fingerprint: String,
}

/// A shared slot the handler writes its verdict into: `check_server_key` can
/// only return a bool, so the reason has to travel out of band.
pub type Slot = Arc<Mutex<Option<Checked>>>;

/// The user's `known_hosts`, scoped to one host and port.
struct KnownHosts {
    host: String,
    port: u16,
}

impl KnownHosts {
    fn new(config: &SshConfig) -> Self {
        Self {
            host: config.host.clone(),
            port: config.port,
        }
    }

    /// Check `key`, learning and trusting an unseen host; failing to persist a
    /// learned key is non-fatal, and only costs re-learning next time.
    fn check(&self, key: &PublicKey) -> Verdict {
        match known_hosts::check_known_hosts(&self.host, self.port, key) {
            Ok(true) => Verdict::Trusted,
            Ok(false) => {
                let _ = known_hosts::learn_known_hosts(&self.host, self.port, key);
                Verdict::Learned
            }
            Err(KeyError::KeyChanged { .. }) => Verdict::Changed,
            Err(_) => Verdict::Unverifiable,
        }
    }
}

/// The `russh` client handler, which accepts a host key per the configured
/// policy and records what the check concluded in its [`Slot`].
pub struct Client {
    known: KnownHosts,
    strict: bool,
    verdict: Slot,
}

impl Client {
    pub fn new(config: &SshConfig, verdict: Slot) -> Self {
        Self {
            known: KnownHosts::new(config),
            strict: config.strict,
            verdict,
        }
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    /// Checked either way: non-strict still records what the check concluded,
    /// so the caller warns rather than accepting a key without a word.
    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        let verdict = self.known.check(key);
        *self.verdict.lock() = Some(Checked {
            verdict,
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        });
        Ok(verdict.accepts(self.strict))
    }
}
