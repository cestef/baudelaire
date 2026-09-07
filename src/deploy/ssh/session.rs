//! The SSH transport: one authenticated connection with an open SFTP session,
//! and the remote directory it reconciles into.

use std::sync::Arc;

use parking_lot::Mutex;

use russh::client::{self, Handle};
use russh::{ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;

use super::auth::Auth;
use super::hosts::{Checked, Client, Verdict};
use crate::config::SshConfig;
use crate::deploy::{Inventory, Listed};
use crate::error::deploy::Step;
use crate::error::warning::{HostKeyAccepted, HostKeyLearned, HostKeyUnverified};
use crate::error::{DeployError, Result};
use crate::remote::Options;
use crate::ui::Ui;

/// A live, authenticated SSH connection with an open SFTP session.
pub struct Session {
    handle: Handle<Client>,
    sftp: SftpSession,
    remote: Remote,
}

impl Session {
    /// Connect, verify the host key, authenticate, and open the SFTP subsystem.
    ///
    /// A host-key warning is flushed as it happens, since warnings are buffered
    /// and the upload would otherwise start before the operator saw it.
    pub async fn connect(
        config: &SshConfig,
        user: &str,
        opts: &Options<'_>,
        ui: &Ui,
    ) -> Result<Self> {
        let rc = Arc::new(client::Config::default());
        let verdict = Arc::new(Mutex::new(None));
        let client = Client::new(config, Arc::clone(&verdict));
        let host = config.host.as_str();
        let mut handle = client::connect(rc, (host, config.port), client)
            .await
            .map_err(|e| match verdict.lock().as_ref().map(|seen| seen.verdict) {
                Some(Verdict::Changed) => DeployError::host_key_changed(host, config.port),
                Some(Verdict::Unverifiable) if config.strict => {
                    DeployError::host_key_unverifiable(host)
                }
                _ => DeployError::connect(host, e),
            })?;
        Self::report(config, verdict.lock().take(), ui);
        Auth::new(config, opts).run(&mut handle, user).await?;

        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| DeployError::connect(host, e))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| DeployError::connect(host, e))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| DeployError::transfer(Step::OpenSftp, e))?;
        Ok(Self {
            handle,
            sftp,
            remote: Remote::new(&config.path),
        })
    }

    /// Say out loud what verifying the host key concluded, so neither a
    /// first-use key nor an unreadable `known_hosts` is trusted in silence.
    fn report(config: &SshConfig, checked: Option<Checked>, ui: &Ui) {
        let Some(checked) = checked else { return };
        match checked.verdict {
            Verdict::Trusted => return,
            Verdict::Learned => ui.warn(HostKeyLearned {
                host: config.host.clone(),
                fingerprint: checked.fingerprint,
            }),
            Verdict::Changed => ui.warn(HostKeyAccepted {
                host: config.host.clone(),
                entry: DeployError::entry(&config.host, config.port),
            }),
            Verdict::Unverifiable => ui.warn(HostKeyUnverified {
                host: config.host.clone(),
                fingerprint: checked.fingerprint,
            }),
        }
        ui.flush();
    }

    /// The remote files' digests, from the host's `sha256sum`; a deploy root
    /// that is not there yet answers with an empty map, so every file reads as
    /// new rather than being skipped wrongly.
    ///
    /// A host that answered with a failure has listed some of its tree at best,
    /// which is safe in one direction only (more uploads, fewer deletes) and so
    /// is warned about rather than passed off as a complete inventory.
    pub async fn digests(&self, ui: &Ui, target: &str) -> Result<Inventory> {
        let (out, status) = self.exec(&self.remote.command()).await?;
        if status != Some(0) {
            ui.warn(crate::error::warning::RemoteListingPartial {
                target: target.to_owned(),
            });
        }
        Ok(Remote::parse(&out))
    }

    /// Upload `body` to the file for dist-relative `rel`, creating parents first.
    ///
    /// Written beside the target and moved onto it, because `create` truncates:
    /// a transfer that dies half way would otherwise leave the live file short
    /// and the host serving it. The rename is the plain SFTP one, which refuses
    /// an existing target, so the target is unlinked first.
    pub async fn upload(&self, rel: &str, body: &[u8]) -> Result<()> {
        let path = self.remote.path(rel);
        let staging = format!("{path}.{}.staging", std::process::id());
        self.mkdirs(&path).await;
        self.write(&staging, body).await?;
        let _ = self.sftp.remove_file(&path).await;
        if let Err(e) = self.sftp.rename(&staging, &path).await {
            let _ = self.sftp.remove_file(&staging).await;
            return Err(DeployError::transfer(Step::Upload, e).into());
        }
        Ok(())
    }

    /// Write `body` to `path`, creating or truncating it.
    async fn write(&self, path: &str, body: &[u8]) -> Result<()> {
        let mut file = self
            .sftp
            .create(path)
            .await
            .map_err(|e| DeployError::transfer(Step::Upload, e))?;
        file.write_all(body)
            .await
            .map_err(|e| DeployError::transfer(Step::Upload, e))?;
        file.flush()
            .await
            .map_err(|e| DeployError::transfer(Step::Upload, e))?;
        file.shutdown()
            .await
            .map_err(|e| DeployError::transfer(Step::Upload, e))?;
        Ok(())
    }

    pub async fn remove(&self, rel: &str) -> Result<()> {
        self.sftp
            .remove_file(self.remote.path(rel))
            .await
            .map_err(|e| DeployError::transfer(Step::Delete, e))?;
        Ok(())
    }

    /// A failed teardown is not worth surfacing.
    pub async fn close(self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await;
    }

    /// Run `command` over an exec channel and collect its stdout and the status
    /// it exited with, capped: the output is the host's own answer, and an
    /// endless stream would otherwise grow this buffer until the process died.
    ///
    /// `None` for the status where the channel ended without one, which is a
    /// transport that died rather than a command that answered.
    async fn exec(&self, command: &str) -> Result<(String, Option<u32>)> {
        const LIMIT: usize = 64 << 20;
        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| DeployError::transfer(Step::Exec, e))?;
        channel
            .exec(true, command)
            .await
            .map_err(|e| DeployError::transfer(Step::Exec, e))?;
        let mut out = Vec::new();
        let mut status = None;
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    if out.len() + data.len() > LIMIT {
                        return Err(DeployError::transfer(
                            Step::Exec,
                            std::io::Error::other(format!(
                                "host sent more than {LIMIT} bytes of output"
                            )),
                        )
                        .into());
                    }
                    out.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok((String::from_utf8_lossy(&out).into_owned(), status))
    }

    /// Ensure every ancestor directory of `path` exists, ignoring the
    /// already-exists error each existing level returns.
    async fn mkdirs(&self, path: &str) {
        let Some((dir, _)) = path.rsplit_once('/') else {
            return;
        };
        let mut prefix = String::new();
        for segment in dir.split('/') {
            prefix.push_str(segment);
            prefix.push('/');
            if !segment.is_empty() {
                let _ = self.sftp.create_dir(prefix.trim_end_matches('/')).await;
            }
        }
    }
}

/// The remote directory the site is mirrored into, mapping dist-relative paths
/// to absolute remote ones.
struct Remote {
    base: String,
}

impl Remote {
    /// The remote root, with any trailing slash trimmed so [`Remote::path`] can
    /// add exactly one; `base` is absolute, as [`super::Ssh::check`] guarantees.
    fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    fn path(&self, rel: &str) -> String {
        format!("{}/{rel}", self.base)
    }

    /// The shell command that lists the tree with a SHA-256 per file.
    /// A deploy root that is not there yet is not a failure: it is a first
    /// deploy, and it answers with nothing rather than with a non-zero status
    /// the caller would report.
    fn command(&self) -> String {
        format!(
            "if cd {} 2>/dev/null; then find . -type f -exec sha256sum {{}} +; fi",
            Self::quote(&self.base)
        )
    }

    /// Single-quote a path for the remote shell, escaping embedded quotes.
    fn quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', r"'\''"))
    }

    /// Parse `sha256sum` output (`<hex>  ./path` per line) into digests keyed
    /// by the relative path.
    ///
    /// A path that would escape the deploy root is refused and *kept*: one
    /// merely dropped is a file the reconcile can neither overwrite nor delete,
    /// and that nothing in the run ever mentions.
    fn parse(output: &str) -> Inventory {
        let mut out = Inventory::default();
        for (hash, path) in output
            .lines()
            .filter_map(|line| line.split_once("  "))
            .filter(|(hash, _)| !hash.is_empty())
        {
            let path = path.trim().trim_start_matches("./");
            match Listed::try_from(path) {
                Ok(listed) => out.admit(listed.into_string(), hash.to_owned()),
                Err(()) => out.refuse(path, crate::deploy::Inventory::OUTSIDE),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_joins_base_and_relative() {
        let remote = Remote::new("/var/www/site/");
        assert_eq!(remote.path("posts/a.html"), "/var/www/site/posts/a.html");
    }

    #[test]
    fn command_quotes_the_base() {
        assert_eq!(
            Remote::new("/srv/o'brien").command(),
            r"if cd '/srv/o'\''brien' 2>/dev/null; then find . -type f -exec sha256sum {} +; fi"
        );
    }

    fn admitted(output: &str) -> crate::deploy::Digests {
        Remote::parse(output).report(&Ui::new(crate::ui::Level::Silent), "host")
    }

    #[test]
    fn parse_reads_sha256sum_output() {
        let out = "\
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  ./index.html
2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae  ./posts/a.css
";
        let digests = admitted(out);
        assert_eq!(digests.len(), 2);
        assert_eq!(
            digests["index.html"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digests["posts/a.css"],
            "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae"
        );
    }

    #[test]
    fn parse_ignores_blank_and_malformed_lines() {
        assert!(admitted("").is_empty());
        assert!(admitted("no-double-space ./x").is_empty());
        assert!(admitted("\n\n").is_empty());
    }

    #[test]
    fn a_refused_remote_path_is_kept_for_reporting() {
        let out = "\
aa  ./index.html
bb  ../../etc/nginx/nginx.conf
cc  ./posts//a.html
";
        let inventory = Remote::parse(out);
        let ui = Ui::new(crate::ui::Level::Silent);
        let files = inventory.report(&ui, "host");
        assert_eq!(files.keys().collect::<Vec<&String>>(), ["index.html"]);
        assert_eq!(ui.warnings(), 1);
    }
}
