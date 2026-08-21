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
use crate::deploy::{Inventory, Listed};
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
        let mut handle = client::connect(rc, (config.host.as_str(), config.port), client)
            .await
            .map_err(|e| {
                let seen = *verdict.lock();
                match seen {
                    Some(Verdict::Changed) => {
                        DeployError::host_key_changed(&config.host, config.port)
                    }
                    _ => DeployError::connect(&config.host, e),
                }
            })?;
        if *verdict.lock() == Some(Verdict::Changed) {
            ui.warn(HostKeyAccepted {
                host: config.host.clone(),
                entry: DeployError::entry(&config.host, config.port),
            });
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

    /// The remote files' digests, from the host's `sha256sum`; a missing
    /// directory or absent tool yields an empty map, so every file reads as new
    /// rather than being skipped wrongly.
    pub async fn digests(&self) -> Result<Inventory> {
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

    /// Run `command` over an exec channel and collect its stdout, capped: the
    /// output is the host's own answer, and an endless stream would otherwise
    /// grow this buffer until the process died.
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
            r"cd '/srv/o'\''brien' && find . -type f -exec sha256sum {} +"
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
