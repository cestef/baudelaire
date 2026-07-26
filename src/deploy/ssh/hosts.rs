//! Host-key verification, and the `russh` client handler that applies it.
//!
//! Strict mode (the default) mirrors OpenSSH: a key already in `known_hosts` is
//! trusted, an unseen host is learned on first connect (trust-on-first-use), and
//! a *changed* key for a known host is refused: the man-in-the-middle guard.
//! Non-strict accepts any key, matching `StrictHostKeyChecking=no`, but still
//! *records* a changed one so the caller can warn: accepting silently makes a
//! flag set once to bootstrap a permanent man-in-the-middle hole.
//!
//! A rejection can't travel out of [`russh::client::Handler::check_server_key`]
//! (it only returns a bool), so a changed key is recorded in a shared [`Slot`]
//! that [`super::session`] reads to raise a precise diagnostic instead of a
//! generic connection failure.

use std::sync::Arc;

use parking_lot::Mutex;

use russh::client;
use russh::keys::known_hosts;
use russh::keys::{Error as KeyError, PublicKey};

use crate::config::SshConfig;

/// The verdict of checking a server key against `known_hosts`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Recorded and matching, or freshly learned: accept.
    Trusted,
    /// Recorded but different: a man-in-the-middle guard trip. A refusal under
    /// `strict`, and the subject of a warning without it.
    Changed,
    /// The file could not be read or parsed: refuse, but without a specific
    /// explanation.
    Unverifiable,
}

/// A shared slot the handler writes its verdict into, read after the connection
/// attempt returns. `check_server_key` can only return a bool, so the reason has
/// to travel out of band.
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

    /// Check `key`, learning and trusting an unseen host (TOFU). A failure to
    /// persist a learned key is non-fatal: it only costs re-learning next time.
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

/// The `russh` client handler: accepts a host key per the configured policy,
/// recording what the check concluded in its [`Slot`].
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

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        // Checked either way: non-strict still records a changed key so the
        // caller warns instead of accepting it without a word.
        let verdict = self.known.check(key);
        *self.verdict.lock() = Some(verdict);
        Ok(!self.strict || verdict == Verdict::Trusted)
    }
}
