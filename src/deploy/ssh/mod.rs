//! An SSH deploy backend on `russh` + `russh-sftp`, reconciling a remote
//! directory with the built `dist`: [`auth`] authenticates, [`hosts`] verifies
//! the server's host key, and [`session`] is the SFTP transport. `russh` is
//! async, so the whole exchange runs on a private current-thread runtime.

mod auth;
mod hosts;
mod session;

use tokio::runtime::{Builder, Runtime};

use super::digest::Digest;
use super::{Backend, Digests, Dist, Store};
use crate::config::SshConfig;
use crate::error::deploy::{Required, Setup};
use crate::error::{DeployError, Result};
use crate::remote::Options;
use crate::ui::Ui;

use self::session::Session;

/// The SSH deploy backend. Holds only config; the connection is opened per run.
pub struct Ssh {
    config: SshConfig,
}

impl Ssh {
    pub fn new(config: SshConfig) -> Self {
        Self { config }
    }

    /// Refuse an `ssh { }` block this backend cannot act on, before a socket is
    /// opened.
    ///
    /// Neither `host` nor `path` has a defensible default: an empty `path`
    /// makes the deploy root `/`, and a relative one is joined as a string and
    /// resolved by the *host*, against whatever directory the SFTP session
    /// happens to start in.
    pub(super) fn check(config: &SshConfig) -> Result<()> {
        if config.host.trim().is_empty() {
            return Err(DeployError::required(Required::SshHost).into());
        }
        let path = config.path.trim();
        if path.is_empty() {
            return Err(DeployError::required(Required::SshPath).into());
        }
        if !path.starts_with('/') {
            return Err(DeployError::Relative {
                path: config.path.clone(),
            }
            .into());
        }
        Ok(())
    }

    /// The user to authenticate as: the configured one, else `login` (the
    /// caller's `$USER`). An error when neither is available, never a fallback
    /// to `root`, which `$USER` being routinely unset in a container or a CI
    /// job would then make the usual case.
    fn user(&self, login: Option<&str>) -> Result<String> {
        match (&self.config.user, login) {
            (Some(user), _) => Ok(user.clone()),
            (None, Some(login)) => Ok(login.to_owned()),
            (None, None) => Err(DeployError::NoUser.into()),
        }
    }

    /// Open one authenticated connection to the configured host. An empty
    /// `$USER` counts as absent: it would authenticate with no username at all.
    fn connect(&self, runtime: &Runtime, opts: &Options<'_>, ui: &Ui) -> Result<Session> {
        let login = std::env::var("USER").ok().filter(|user| !user.is_empty());
        let user = self.user(login.as_deref())?;
        runtime.block_on(Session::connect(&self.config, &user, opts, ui))
    }
}

impl Backend<Dist> for Ssh {
    fn name(&self) -> &'static str {
        "ssh"
    }

    fn run(&self, dist: &Dist, opts: &Options<'_>, ui: &Ui) -> Result<()> {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| DeployError::local(Setup::Runtime, e))?;
        let sftp = Sftp {
            runtime: &runtime,
            session: self.connect(&runtime, opts, ui)?,
            config: &self.config,
        };
        let result = dist.reconcile(&sftp, self.config.delete, opts, ui);
        if result.is_ok() {
            sftp.close();
        }
        result
    }
}

/// The live SSH session as a blocking [`Store`], each operation driving the
/// runtime the backend owns.
struct Sftp<'a> {
    runtime: &'a Runtime,
    session: Session,
    config: &'a SshConfig,
}

impl Sftp<'_> {
    fn close(self) {
        self.runtime.block_on(self.session.close());
    }
}

impl Store for Sftp<'_> {
    /// One SFTP session, one request at a time.
    fn concurrency(&self) -> Option<usize> {
        Some(1)
    }

    /// The host hashes its own files with `sha256sum`, so the local side must
    /// match it exactly.
    fn digest(&self, bytes: &[u8]) -> String {
        Digest::sha256(bytes)
    }

    fn list(&self, ui: &Ui) -> Result<Digests> {
        let inventory = self.runtime.block_on(self.session.digests())?;
        Ok(inventory.report(ui, &self.target()))
    }

    fn upload(&self, key: &str, body: &[u8]) -> Result<()> {
        self.runtime.block_on(self.session.upload(key, body))
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.runtime.block_on(self.session.remove(key))
    }

    fn target(&self) -> String {
        format!("{}:{}", self.config.host, self.config.path)
    }
}

#[cfg(test)]
mod tests {
    use super::Ssh;
    use crate::config::SshConfig;

    #[test]
    fn a_missing_user_is_an_error_not_root() {
        let err = Ssh::new(SshConfig::default()).user(None).unwrap_err();
        let err = err.to_string();
        assert!(err.contains("no ssh user"), "{err}");
        assert!(!err.contains("root"), "{err}");
    }

    #[test]
    fn a_block_without_a_host_or_an_absolute_path_is_refused() {
        let block = |host: &str, path: &str| SshConfig {
            host: host.into(),
            path: path.into(),
            ..SshConfig::default()
        };
        assert!(Ssh::check(&block("srv", "/var/www/site")).is_ok());
        assert!(Ssh::check(&block("", "/var/www/site")).is_err(), "no host");
        assert!(Ssh::check(&block("srv", "")).is_err(), "no path");
        assert!(Ssh::check(&block("srv", "  ")).is_err(), "a blank path");
        assert!(Ssh::check(&block("srv", "var/www")).is_err(), "relative");
        assert!(Ssh::check(&block("srv", "~/site")).is_err(), "a `~` path");
    }

    #[test]
    fn a_configured_user_wins_over_the_login() {
        let ssh = Ssh::new(SshConfig {
            user: Some("deploy".into()),
            ..SshConfig::default()
        });
        assert_eq!(ssh.user(Some("ada")).unwrap(), "deploy");
        assert_eq!(
            Ssh::new(SshConfig::default()).user(Some("ada")).unwrap(),
            "ada"
        );
    }
}
