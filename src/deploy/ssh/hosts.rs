//! Host-key verification, and the `russh` client handler that applies it:
//! strict mirrors OpenSSH, non-strict accepts any key but records a change.

use std::sync::Arc;

use parking_lot::Mutex;

use russh::client;
use russh::keys::known_hosts;
use russh::keys::{Error as KeyError, PublicKey};

use crate::config::SshConfig;

/// The verdict of checking a server key against `known_hosts`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Recorded and matching, or freshly learned.
    Trusted,
    /// Recorded but different: refused under `strict`, warned about without it.
    Changed,
    /// The file could not be read or parsed.
    Unverifiable,
}

/// A shared slot the handler writes its verdict into: `check_server_key` can
/// only return a bool, so the reason has to travel out of band.
pub type Slot = Arc<Mutex<Option<Verdict>>>;

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
                Verdict::Trusted
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

    /// Checked either way: non-strict still records a changed key, so the
    /// caller warns rather than accepting it without a word.
    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        let verdict = self.known.check(key);
        *self.verdict.lock() = Some(verdict);
        Ok(!self.strict || verdict == Verdict::Trusted)
    }
}
