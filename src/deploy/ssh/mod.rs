//! An SSH deploy backend on `russh` + `russh-sftp` — pure Rust, no shelling out.
//! It reconciles a remote directory with the built `dist`: upload what changed,
//! delete what the build no longer produces.
//!
//! The work is split into cohesive pieces: [`auth`] resolves and performs
//! authentication (key, agent, password), [`hosts`] verifies the server's host
//! key, and [`session`] is the transport that lists, uploads, and deletes over
//! SFTP. Change detection is stateless and content-correct — the host hashes its
//! files with `sha256sum`, diffed against the local files' SHA-256. `russh` is
//! async, so the whole exchange runs on a private current-thread runtime, keeping
//! the rest of the codebase blocking.

mod auth;
mod hosts;
mod session;

use tokio::runtime::Builder;

use super::sigv4::Signer;
use super::{Backend, Digests, Dist, Plan};
use crate::config::SshConfig;
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

    /// The user to authenticate as: the configured one, else `$USER`.
    fn user(&self) -> String {
        self.config
            .user
            .clone()
            .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".into()))
    }

    /// Reconcile the remote directory with `dist` over one connection.
    async fn sync(&self, dist: &Dist, local: &Digests, opts: &Options<'_>, ui: &Ui) -> Result<()> {
        let session = Session::connect(&self.config, &self.user(), opts).await?;
        let plan = Plan::compute(local, &session.digests().await?, self.config.delete);
        plan.preview(ui, opts.dry_run);
        if opts.dry_run {
            session.close().await;
            return Ok(());
        }
        for key in &plan.uploads {
            session.upload(key, &dist.read(key)?).await?;
            ui.item(format_args!("↑ {key}"));
        }
        for key in &plan.deletes {
            session.remove(key).await?;
            ui.item(format_args!("✕ {key}"));
        }
        session.close().await;
        plan.done(ui, format_args!("{}:{}", self.config.host, self.config.path));
        Ok(())
    }
}

impl Backend for Ssh {
    fn name(&self) -> &'static str {
        "ssh"
    }

    fn run(&self, dist: &Dist, opts: &Options<'_>, ui: &Ui) -> Result<()> {
        // Hashing is blocking (file reads); do it up front, then drive the async
        // network exchange on a private runtime so async never leaks outward.
        let local = dist.digests(Signer::sha256_hex)?;
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| DeployError::transfer("start runtime", e))?;
        runtime.block_on(self.sync(dist, &local, opts, ui))
    }
}
