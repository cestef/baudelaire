//! Host-key verification, and the `russh` client handler that applies it.
//!
//! Strict mode (the default) mirrors OpenSSH: a key already in `known_hosts` is
//! trusted, an unseen host is learned on first connect (trust-on-first-use), and
//! a *changed* key for a known host is refused: the man-in-the-middle guard.
//! Non-strict accepts any key, matching `StrictHostKeyChecking=no`.
//!
//! A rejection can't travel out of [`russh::client::Handler::check_server_key`]
//! (it only returns a bool), so a changed key is recorded in a shared [`Slot`]
//! that [`super::session`] reads to raise a precise diagnostic instead of a
//! generic connection failure.

use std::sync::{Arc, Mutex};

use russh::client;
use russh::keys::known_hosts;
use russh::keys::{Error as KeyError, PublicKey};

use crate::config::SshConfig;

/// Why a server key was refused: the case worth an actionable message.
#[derive(Clone, Copy)]
pub enum Rejected {
    /// A key is recorded for this host, but the server presented a different one.
    Changed,
}

/// A shared slot the handler writes its rejection reason into, read after the
/// connection attempt returns.
pub type Slot = Arc<Mutex<Option<Rejected>>>;

/// The verdict of checking a server key against `known_hosts`.
enum Verdict {
    /// Recorded and matching, or freshly learned: accept.
    Trusted,
    /// Recorded but different: a man-in-the-middle guard trip.
    Changed,
    /// The file could not be read or parsed: refuse, but without a specific
    /// explanation.
    Unverifiable,
}

/// The user's `known_hosts`, scoped to one host and port.
struct KnownHosts {
    host: String,
    port: u16,
}

impl KnownHosts {
    fn new(config: &SshConfig) -> Self {
        Self { host: config.host.clone(), port: config.port }
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
/// recording a changed-key rejection in its [`Slot`].
pub struct Client {
    known: KnownHosts,
    strict: bool,
    rejected: Slot,
}

impl Client {
    pub fn new(config: &SshConfig, rejected: Slot) -> Self {
        Self { known: KnownHosts::new(config), strict: config.strict, rejected }
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        if !self.strict {
            return Ok(true);
        }
        match self.known.check(key) {
            Verdict::Trusted => Ok(true),
            Verdict::Changed => {
                *self.rejected.lock().unwrap() = Some(Rejected::Changed);
                Ok(false)
            }
            Verdict::Unverifiable => Ok(false),
        }
    }
}
