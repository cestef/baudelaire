//! The SSH transport: one authenticated connection with an open SFTP session,
//! and the remote directory it reconciles into.

use std::sync::Arc;

use parking_lot::Mutex;

use russh::client::{self, Handle};
use russh::{ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;

use super::auth::Auth;
use super::hosts::{Client, Verdict};
use crate::config::SshConfig;
use crate::deploy::{Digests, Listed};
use crate::error::deploy::Step;
use crate::error::warning::HostKeyAccepted;
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
    pub async fn connect(
        config: &SshConfig,
        user: &str,
        opts: &Options<'_>,
        ui: &Ui,
    ) -> Result<Self> {
        let rc = Arc::new(client::Config::default());
        // The handler records its verdict here, since it can only return a
        // bool; a plain connection failure otherwise stays generic.
        let verdict = Arc::new(Mutex::new(None));
        let client = Client::new(config, Arc::clone(&verdict));
        let mut handle = client::connect(rc, (config.host.as_str(), config.port), client)
            .await
            .map_err(|e| {
                // Read the verdict out before matching: the guard must not stay
                // alive for the arms, which lock nothing but would hold it.
                let seen = *verdict.lock();
                match seen {
                    Some(Verdict::Changed) => DeployError::host_key_changed(&config.host),
                    _ => DeployError::connect(&config.host, e),
                }
            })?;
        // Connected despite a changed key: only `strict #false` gets here, and
        // it must not do so quietly.
        if *verdict.lock() == Some(Verdict::Changed) {
            ui.warn(HostKeyAccepted {
                host: config.host.clone(),
            });
            // Rendered now, not at the end of the run: warnings are buffered,
            // and every file would otherwise be uploaded to the new host before
            // the operator was told the key had changed.
            ui.flush();
        }
        Auth::new(config, opts).run(&mut handle, user).await?;

        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| DeployError::connect(&config.host, e))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| DeployError::connect(&config.host, e))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| DeployError::transfer(Step::OpenSftp, e))?;
        Ok(Self {
            handle,
            sftp,
            remote: Remote::new(&config.path),
        })
    }

    /// The remote files' digests, from the host's `sha256sum`. A missing
    /// directory or absent tool yields an empty map, so every file reads as new
    /// and the site uploads in full rather than skipping wrongly.
    pub async fn digests(&self) -> Result<Digests> {
        Ok(Remote::parse(&self.exec(&self.remote.command()).await?))
    }

    /// Upload `body` to the file for dist-relative `rel`, creating parents first.
    pub async fn upload(&self, rel: &str, body: &[u8]) -> Result<()> {
        let path = self.remote.path(rel);
        self.mkdirs(&path).await;
        let mut file = self
            .sftp
            .create(&path)
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

    /// Delete the remote file for dist-relative `rel`.
    pub async fn remove(&self, rel: &str) -> Result<()> {
        self.sftp
            .remove_file(self.remote.path(rel))
            .await
            .map_err(|e| DeployError::transfer(Step::Delete, e))?;
        Ok(())
    }

    /// Cleanly disconnect; a failed teardown is not worth surfacing.
    pub async fn close(self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await;
    }

    /// Run `command` over an exec channel and collect its stdout.
    ///
    /// Capped: the output is the host's own answer, and an endless stream (a
    /// hostile host, or a `find` pointed at something enormous) would otherwise
    /// grow this buffer until the process died. The same ceiling reasoning as
    /// `atproto::Repo`'s page limit. 64 MiB of `sha256sum` output is roughly a
    /// million files, far past any site this reconciles.
    async fn exec(&self, command: &str) -> Result<String> {
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
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
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

/// The remote directory the site is mirrored into: it maps dist-relative paths to
/// absolute remote ones and speaks the `sha256sum` listing protocol.
struct Remote {
    base: String,
}

impl Remote {
    fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    /// The absolute remote path for a dist-relative file.
    fn path(&self, rel: &str) -> String {
        format!("{}/{rel}", self.base)
    }

    /// The shell command that lists the tree with a SHA-256 per file.
    fn command(&self) -> String {
        format!(
            "cd {} && find . -type f -exec sha256sum {{}} +",
            Self::quote(&self.base)
        )
    }

    /// Single-quote a path for the remote shell, escaping embedded quotes.
    fn quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', r"'\''"))
    }

    /// Parse `sha256sum` output (`<hex>  ./path` per line) into digests keyed by
    /// the relative path, dropping the `./` prefix `find` emits.
    ///
    /// Paths that would escape the deploy root are dropped here, the one place
    /// the host's own answer becomes a path this client joins and deletes: see
    /// [`Listed`].
    fn parse(output: &str) -> Digests {
        output
            .lines()
            .filter_map(|line| line.split_once("  "))
            .filter(|(hash, _)| !hash.is_empty())
            .filter_map(|(hash, path)| {
                let path = Listed::try_from(path.trim().trim_start_matches("./")).ok()?;
                Some((path.into_string(), hash.to_owned()))
            })
            .collect()
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
            r"cd '/srv/o'\''brien' && find . -type f -exec sha256sum {} +"
        );
    }

    #[test]
    fn parse_reads_sha256sum_output() {
        let out = "\
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  ./index.html
2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae  ./posts/a.css
";
        let digests = Remote::parse(out);
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
        assert!(Remote::parse("").is_empty());
        assert!(Remote::parse("no-double-space ./x").is_empty());
        assert!(Remote::parse("\n\n").is_empty());
    }
}
