//! SSH authentication: a configured private key is used exclusively, and
//! without one the agent's identities are offered before a password.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh::client::AuthResult;
use russh::client::Handle;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::{Error as KeyError, HashAlg, PrivateKey, PrivateKeyWithHashAlg, load_secret_key};
use tokio::io::{AsyncRead, AsyncWrite};

use super::hosts::Client;
use crate::config::SshConfig;
use crate::error::deploy::{Setup, Step};
use crate::error::{DeployError, Result};
use crate::remote::Options;

/// Environment variable for the SSH secret: a password (no key), or the
/// passphrase of an encrypted key.
pub const PASSWORD_ENV: &str = crate::config::Secrets::SSH_PASSWORD;

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
        let hash = handle
            .best_supported_rsa_hash()
            .await
            .ok()
            .flatten()
            .flatten();
        let ok = if self.config.key.is_some() {
            self.key(handle, user, hash).await?
        } else {
            self.agent(handle, user, hash).await? || self.password(handle, user).await?
        };
        ok.then_some(()).ok_or_else(|| {
            DeployError::Auth {
                user: user.to_owned(),
            }
            .into()
        })
    }

    async fn key(
        &self,
        handle: &mut Handle<Client>,
        user: &str,
        hash: Option<HashAlg>,
    ) -> Result<bool> {
        let key = Arc::new(self.load()?);
        Self::ok(
            handle
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(key, hash))
                .await,
        )
    }

    /// Any agent hiccup (no socket, no keys, a rejected identity) yields
    /// `false`, so the caller falls back to a password.
    async fn agent(
        &self,
        handle: &mut Handle<Client>,
        user: &str,
        hash: Option<HashAlg>,
    ) -> Result<bool> {
        Agent::offer(handle, user, hash).await
    }

    /// Authenticate with a password from the environment, stdin, or prompt.
    async fn password(&self, handle: &mut Handle<Client>, user: &str) -> Result<bool> {
        let password = self.opts.secret(PASSWORD_ENV, "ssh password")?;
        Self::ok(handle.authenticate_password(user, password).await)
    }

    fn ok(result: Result<AuthResult, russh::Error>) -> Result<bool> {
        Ok(result
            .map_err(|e| DeployError::transfer(Step::Authenticate, e))?
            .success())
    }

    /// Load the configured private key, prompting for a passphrase only if the
    /// key turns out to be encrypted.
    fn load(&self) -> Result<PrivateKey> {
        let key = self.config.key.as_ref().expect("key configured");
        let path = Self::expand(key, std::env::var_os("HOME"));
        match load_secret_key(&path, None) {
            Ok(key) => Ok(key),
            Err(KeyError::IO(why)) => Err(Self::unreadable(&path, why)),
            Err(KeyError::KeyIsEncrypted) => {
                let passphrase = self.opts.secret(PASSWORD_ENV, "ssh key passphrase")?;
                load_secret_key(&path, Some(&passphrase))
                    .map_err(|e| DeployError::local(Setup::PrivateKey, e).into())
            }
            Err(why) => Err(DeployError::local(Setup::PrivateKey, why).into()),
        }
    }

    /// Which of the two file-level failures this is: they want different
    /// answers (fix the path, or fix the mode) and neither wants a passphrase.
    fn unreadable(path: &Path, why: std::io::Error) -> crate::error::BaudelaireErrorKind {
        let path = path.display().to_string();
        match why.kind() {
            std::io::ErrorKind::NotFound => DeployError::KeyMissing { path },
            _ => DeployError::KeyUnreadable { path, source: why },
        }
        .into()
    }

    /// Expand a leading `~` against `home`, leaving other paths untouched, and
    /// a `~` path unchanged when there is no home to resolve.
    fn expand(path: &Path, home: Option<std::ffi::OsString>) -> PathBuf {
        match (path.strip_prefix("~"), home) {
            (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
            _ => path.to_owned(),
        }
    }
}

/// The ssh-agent, and the one thing about reaching it that is not portable:
/// where it listens.
struct Agent;

impl Agent {
    /// Offer every identity the agent holds, `true` as soon as one is accepted.
    #[cfg(unix)]
    async fn offer(handle: &mut Handle<Client>, user: &str, hash: Option<HashAlg>) -> Result<bool> {
        match AgentClient::connect_env().await {
            Ok(mut agent) => Self::identities(&mut agent, handle, user, hash).await,
            Err(_) => Ok(false),
        }
    }

    /// OpenSSH's named pipe is what a `ssh` on `PATH` talks to, so Pageant is
    /// only tried after it.
    #[cfg(windows)]
    async fn offer(handle: &mut Handle<Client>, user: &str, hash: Option<HashAlg>) -> Result<bool> {
        const PIPE: &str = r"\\.\pipe\openssh-ssh-agent";
        if let Ok(mut agent) = AgentClient::connect_named_pipe(PIPE).await
            && Self::identities(&mut agent, handle, user, hash).await?
        {
            return Ok(true);
        }
        match AgentClient::connect_pageant().await {
            Ok(mut agent) => Self::identities(&mut agent, handle, user, hash).await,
            Err(_) => Ok(false),
        }
    }

    /// Offer each identity a connected agent holds.
    async fn identities<S>(
        agent: &mut AgentClient<S>,
        handle: &mut Handle<Client>,
        user: &str,
        hash: Option<HashAlg>,
    ) -> Result<bool>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        let Ok(identities) = agent.request_identities().await else {
            return Ok(false);
        };
        for identity in identities {
            if let AgentIdentity::PublicKey { key, .. } = identity
                && let Ok(result) = handle
                    .authenticate_publickey_with(user, key, hash, agent)
                    .await
                && result.success()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BaudelaireErrorKind;

    #[test]
    fn a_key_that_was_never_read_is_not_called_encrypted() {
        let key = Path::new("/does/not/exist/id_ed25519");
        assert!(matches!(
            Auth::unreadable(key, std::io::Error::from(std::io::ErrorKind::NotFound)),
            BaudelaireErrorKind::Deploy(DeployError::KeyMissing { .. })
        ));
        assert!(matches!(
            Auth::unreadable(
                key,
                std::io::Error::from(std::io::ErrorKind::PermissionDenied)
            ),
            BaudelaireErrorKind::Deploy(DeployError::KeyUnreadable { .. })
        ));
    }

    #[test]
    fn expand_replaces_leading_tilde_with_home() {
        let home = Some("/home/test".into());
        assert_eq!(
            Auth::expand(Path::new("~/.ssh/id_ed25519"), home.clone()),
            PathBuf::from("/home/test/.ssh/id_ed25519")
        );
        assert_eq!(
            Auth::expand(Path::new("/etc/key"), home),
            PathBuf::from("/etc/key")
        );
        assert_eq!(Auth::expand(Path::new("~/k"), None), PathBuf::from("~/k"));
    }
}
