//! SSH authentication: an explicit private key, the ssh-agent, or a password,
//! tried in that order. A configured key is used exclusively; without one the
//! agent is offered every identity it holds before falling back to a password.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh::client::Handle;
use russh::client::AuthResult;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, load_secret_key};

use super::hosts::Client;
use crate::config::SshConfig;
use crate::error::{DeployError, Result};
use crate::remote::Options;

/// Environment variable for the SSH secret — a password (no key), or the
/// passphrase of an encrypted key.
pub const PASSWORD_ENV: &str = "BAUDELAIRE_SSH_PASSWORD";

/// Authenticates a connection against the config and environment.
pub struct Auth<'a> {
    config: &'a SshConfig,
    opts: &'a Options<'a>,
}

impl<'a> Auth<'a> {
    pub fn new(config: &'a SshConfig, opts: &'a Options<'a>) -> Self {
        Self { config, opts }
    }

    /// Authenticate `handle` as `user`, erroring only if every applicable method
    /// is exhausted without success.
    pub async fn run(&self, handle: &mut Handle<Client>, user: &str) -> Result<()> {
        // RSA keys need a negotiated SHA-2 hash; other key types ignore it.
        let hash = handle.best_supported_rsa_hash().await.ok().flatten().flatten();
        let ok = if self.config.key.is_some() {
            self.key(handle, user, hash).await?
        } else {
            self.agent(handle, user, hash).await? || self.password(handle, user).await?
        };
        ok.then_some(())
            .ok_or_else(|| DeployError::Auth { user: user.to_owned() }.into())
    }

    /// Authenticate with the configured private key.
    async fn key(&self, handle: &mut Handle<Client>, user: &str, hash: Option<HashAlg>) -> Result<bool> {
        let key = Arc::new(self.load()?);
        Self::ok(handle.authenticate_publickey(user, PrivateKeyWithHashAlg::new(key, hash)).await)
    }

    /// Authenticate through the ssh-agent, offering each identity it holds. Any
    /// agent hiccup (no socket, no keys, a rejected identity) simply yields
    /// `false`, so the caller falls back to a password.
    async fn agent(&self, handle: &mut Handle<Client>, user: &str, hash: Option<HashAlg>) -> Result<bool> {
        let Ok(mut agent) = AgentClient::connect_env().await else {
            return Ok(false);
        };
        let Ok(identities) = agent.request_identities().await else {
            return Ok(false);
        };
        for identity in identities {
            if let AgentIdentity::PublicKey { key, .. } = identity
                && let Ok(result) = handle.authenticate_publickey_with(user, key, hash, &mut agent).await
                && result.success()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Authenticate with a password from the environment, stdin, or prompt.
    async fn password(&self, handle: &mut Handle<Client>, user: &str) -> Result<bool> {
        let password = self.opts.secret(PASSWORD_ENV, "ssh password")?;
        Self::ok(handle.authenticate_password(user, password).await)
    }

    /// Whether an authentication attempt succeeded, mapping a transport failure.
    fn ok(result: Result<AuthResult, russh::Error>) -> Result<bool> {
        Ok(result.map_err(|e| DeployError::transfer("authenticate", e))?.success())
    }

    /// Load the configured private key, prompting for a passphrase only if the
    /// key turns out to be encrypted.
    fn load(&self) -> Result<PrivateKey> {
        let key = self.config.key.as_ref().expect("key configured");
        let path = Self::expand(key, std::env::var_os("HOME"));
        match load_secret_key(&path, None) {
            Ok(key) => Ok(key),
            Err(_) => {
                let passphrase = self.opts.secret(PASSWORD_ENV, "ssh key passphrase")?;
                load_secret_key(&path, Some(&passphrase))
                    .map_err(|e| DeployError::transfer("load key", e).into())
            }
        }
    }

    /// Expand a leading `~` in a key path against `home`, leaving other paths
    /// untouched (and a `~` path unchanged when there is no home to resolve).
    fn expand(path: &Path, home: Option<std::ffi::OsString>) -> PathBuf {
        match (path.strip_prefix("~"), home) {
            (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
            _ => path.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_replaces_leading_tilde_with_home() {
        let home = Some("/home/test".into());
        assert_eq!(Auth::expand(Path::new("~/.ssh/id_ed25519"), home.clone()), PathBuf::from("/home/test/.ssh/id_ed25519"));
        assert_eq!(Auth::expand(Path::new("/etc/key"), home), PathBuf::from("/etc/key"));
        // No home to resolve against: the `~` path is left as-is.
        assert_eq!(Auth::expand(Path::new("~/k"), None), PathBuf::from("~/k"));
    }
}
