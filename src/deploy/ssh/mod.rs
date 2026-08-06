//! An SSH deploy backend on `russh` + `russh-sftp`: pure Rust, no shelling out.
//! It reconciles a remote directory with the built `dist`: upload what changed,
//! delete what the build no longer produces.
//!
//! The work is split into cohesive pieces: [`auth`] resolves and performs
//! authentication (key, agent, password), [`hosts`] verifies the server's host
//! key, and [`session`] is the transport that lists, uploads, and deletes over
//! SFTP. Change detection is stateless and content-correct: the host hashes its
//! files with `sha256sum`, diffed against the local files' SHA-256. `russh` is
//! async, so the whole exchange runs on a private current-thread runtime, keeping
//! the rest of the codebase blocking.

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
    /// Neither `host` nor `path` has a defensible default, and both defaulted
    /// to the empty string: `deploy { ssh { host "srv" } }` parsed, and the
    /// empty `path` made the deploy root `/`, so `index.html` went to
    /// `/index.html` and the run issued `create_dir("/assets")` on the host as
    /// whichever user it had authenticated as. A relative `path` is refused
    /// too: it is joined as a string and resolved by the *host*, against
    /// whatever directory the SFTP session happens to start in.
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
    /// caller's `$USER`), passed in so the rule is testable without touching
    /// the process environment.
    ///
    /// An error when neither is available, rather than the old fallback to
    /// **`root`**: in a container, a CI job or a systemd unit `$USER` is
    /// routinely unset, and the docs and the config both promise `$USER`, so
    /// that was an undocumented root login attempt nobody asked for.
    fn user(&self, login: Option<&str>) -> Result<String> {
        match (&self.config.user, login) {
            (Some(user), _) => Ok(user.clone()),
            (None, Some(login)) => Ok(login.to_owned()),
            (None, None) => Err(DeployError::NoUser.into()),
        }
    }

    /// Open one authenticated connection to the configured host.
    fn connect(&self, runtime: &Runtime, opts: &Options<'_>, ui: &Ui) -> Result<Session> {
        // An empty `$USER` is as absent as an unset one: it would authenticate
        // with no username at all.
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
        // `russh` is async and nothing else here is, so the whole exchange runs
        // on a private runtime and async never leaks outward.
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
        // Disconnect cleanly when the run got that far; a failed one leaves the
        // connection to drop with the process, since there is nothing left to
        // say to a host that just refused something.
        if result.is_ok() {
            sftp.close();
        }
        result
    }
}

/// The live SSH session as a blocking [`Store`]: each operation drives the
/// runtime the backend owns, so the reconcile loop above it is the same one the
/// S3 backend runs.
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

    /// An absent `$USER` with no configured `user` is an error, not a silent
    /// root login: containers, CI jobs and systemd units routinely have no
    /// `$USER`, and both the docs and the config say it defaults to it.
    #[test]
    fn a_missing_user_is_an_error_not_root() {
        let err = Ssh::new(SshConfig::default()).user(None).unwrap_err();
        let err = err.to_string();
        assert!(err.contains("no ssh user"), "{err}");
        assert!(!err.contains("root"), "{err}");
    }

    /// `host` and `path` have no defensible default, and both defaulted to the
    /// empty string. The empty `path` is the dangerous half: it made the deploy
    /// root `/`.
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
