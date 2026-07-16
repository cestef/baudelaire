//! Host-key verification, and the `russh` client handler that applies it.
//!
//! Strict mode (the default) mirrors OpenSSH: a key already in `known_hosts` is
//! trusted, an unseen host is learned on first connect (trust-on-first-use), and
//! a *changed* key for a known host is refused — the man-in-the-middle guard.
//! Non-strict accepts any key, matching `StrictHostKeyChecking=no`.

use russh::client;
use russh::keys::PublicKey;
use russh::keys::known_hosts;

use crate::config::SshConfig;

/// The user's `known_hosts`, scoped to one host and port.
struct KnownHosts {
    host: String,
    port: u16,
}

impl KnownHosts {
    fn new(config: &SshConfig) -> Self {
        Self { host: config.host.clone(), port: config.port }
    }

    /// Whether `key` is the trusted key for this host — trusting and recording an
    /// unseen host (TOFU), rejecting a changed one. A failure to persist the
    /// learned key is non-fatal: it only costs re-learning next time.
    fn trusts(&self, key: &PublicKey) -> bool {
        match known_hosts::check_known_hosts(&self.host, self.port, key) {
            Ok(true) => true,
            Ok(false) => {
                let _ = known_hosts::learn_known_hosts(&self.host, self.port, key);
                true
            }
            Err(_) => false,
        }
    }
}

/// The `russh` client handler: accepts a host key per the configured policy.
pub struct Client {
    known: KnownHosts,
    strict: bool,
}

impl Client {
    pub fn new(config: &SshConfig) -> Self {
        Self { known: KnownHosts::new(config), strict: config.strict }
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(!self.strict || self.known.trusts(key))
    }
}
