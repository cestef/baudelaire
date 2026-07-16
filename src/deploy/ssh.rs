//! An SSH deploy backend on `russh` + `russh-sftp` — pure Rust, no shelling out.
//! It reconciles a remote directory with the built `dist`: upload what changed,
//! delete what the build no longer produces.
//!
//! Change detection is stateless and content-correct: the host's `sha256sum`
//! computes the remote digests, diffed against the local files' SHA-256. If the
//! host can't run it the remote reads as empty, so the worst case is re-uploading
//! — never a wrong skip. Files move over SFTP; `russh` is async, so the whole
//! network exchange runs on a private current-thread runtime, keeping the rest of
//! the codebase blocking.

use std::collections::BTreeMap;
use std::sync::Arc;

use russh::client::{self, Handle};
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, load_secret_key};
use russh::{ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;
use tokio::runtime::Runtime;

use super::sigv4::sha256_hex;
use super::{Backend, Dist, plan, report};
use crate::config::SshConfig;
use crate::error::{DeployError, Result};
use crate::remote::Options;
use crate::ui::Ui;

/// Environment variable for the SSH secret — a password (no key configured) or a
/// key passphrase (encrypted key).
pub const PASSWORD_ENV: &str = "BAUDELAIRE_SSH_PASSWORD";

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

    /// Resolve the credential: load the configured private key (prompting for a
    /// passphrase only if it is encrypted), else a password from the environment
    /// or prompt.
    fn credential(&self, opts: &Options<'_>) -> Result<Credential> {
        let Some(key) = &self.config.key else {
            return Ok(Credential::Password(opts.secret(PASSWORD_ENV, "ssh password")?));
        };
        match load_secret_key(key, None) {
            Ok(loaded) => Ok(Credential::Key(Box::new(loaded))),
            Err(_) => {
                let passphrase = opts.secret(PASSWORD_ENV, "ssh key passphrase")?;
                let loaded = load_secret_key(key, Some(&passphrase))
                    .map_err(|e| DeployError::connect(&self.config.host, e))?;
                Ok(Credential::Key(Box::new(loaded)))
            }
        }
    }

    /// Reconcile the remote directory with `dist` over one connection.
    async fn sync(&self, dist: &Dist, local: &Digests, cred: Credential, opts: &Options<'_>, ui: &Ui) -> Result<()> {
        let session = Session::connect(&self.config, &self.user(), cred).await?;
        let remote = session.digests(&self.config.path).await?;
        let plan = plan(local, &remote, self.config.delete);
        report(ui, &plan, opts.dry_run);
        if opts.dry_run {
            session.close().await;
            return Ok(());
        }
        for key in &plan.uploads {
            session.upload(&self.config.path, key, &dist.read(key)?).await?;
            ui.item(format_args!("↑ {key}"));
        }
        for key in &plan.deletes {
            session.remove(&self.config.path, key).await?;
            ui.item(format_args!("✕ {key}"));
        }
        session.close().await;
        ui.done(format_args!(
            "deployed to {}:{} — {} uploaded, {} deleted",
            self.config.host,
            self.config.path,
            plan.uploads.len(),
            plan.deletes.len()
        ));
        Ok(())
    }
}

impl Backend for Ssh {
    fn name(&self) -> &'static str {
        "ssh"
    }

    fn run(&self, dist: &Dist, opts: &Options<'_>, ui: &Ui) -> Result<()> {
        // Credential resolution and hashing are blocking (prompts, file reads);
        // do them up front, then run the async network exchange on a private
        // runtime so async never leaks into the rest of the codebase.
        let cred = self.credential(opts)?;
        let local = dist.digests(sha256_hex)?;
        runtime()?.block_on(self.sync(dist, &local, cred, opts, ui))
    }
}

/// The digests of every built file, keyed by dist-relative path.
type Digests = BTreeMap<String, String>;

/// How to authenticate: a loaded private key, or a password.
enum Credential {
    Key(Box<PrivateKey>),
    Password(String),
}

/// A live SSH connection with an open SFTP session.
struct Session {
    handle: Handle<Client>,
    sftp: SftpSession,
}

impl Session {
    /// Connect, authenticate, and open the SFTP subsystem.
    async fn connect(config: &SshConfig, user: &str, cred: Credential) -> Result<Self> {
        let rc = Arc::new(client::Config::default());
        let mut handle = client::connect(rc, (config.host.as_str(), config.port), Client)
            .await
            .map_err(|e| DeployError::connect(&config.host, e))?;

        let authenticated = match cred {
            Credential::Password(password) => handle
                .authenticate_password(user, password)
                .await
                .map_err(|e| DeployError::connect(&config.host, e))?,
            Credential::Key(key) => {
                let hash = handle.best_supported_rsa_hash().await.ok().flatten().flatten();
                handle
                    .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(*key), hash))
                    .await
                    .map_err(|e| DeployError::connect(&config.host, e))?
            }
        };
        if !authenticated.success() {
            return Err(DeployError::Auth { user: user.to_owned() }.into());
        }

        let channel = handle.channel_open_session().await.map_err(|e| DeployError::connect(&config.host, e))?;
        channel.request_subsystem(true, "sftp").await.map_err(|e| DeployError::connect(&config.host, e))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| DeployError::transfer("open sftp", e))?;
        Ok(Self { handle, sftp })
    }

    /// The remote files' digests under `base`, from the host's `sha256sum`. A
    /// missing directory or absent tool yields an empty map — every file then
    /// reads as new, so the site uploads in full rather than skipping wrongly.
    async fn digests(&self, base: &str) -> Result<Digests> {
        let out = self.exec(&format!("cd {} && find . -type f -exec sha256sum {{}} +", quote(base))).await?;
        Ok(parse(&out))
    }

    /// Run a command over an exec channel and collect its stdout.
    async fn exec(&self, command: &str) -> Result<String> {
        let mut channel = self.handle.channel_open_session().await.map_err(|e| DeployError::transfer("exec", e))?;
        channel.exec(true, command).await.map_err(|e| DeployError::transfer("exec", e))?;
        let mut out = Vec::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => out.extend_from_slice(&data),
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// Upload `body` to `base/rel`, creating parent directories first.
    async fn upload(&self, base: &str, rel: &str, body: &[u8]) -> Result<()> {
        let path = join(base, rel);
        self.mkdirs(&path).await;
        let mut file = self.sftp.create(&path).await.map_err(|e| DeployError::transfer("upload", e))?;
        file.write_all(body).await.map_err(|e| DeployError::transfer("upload", e))?;
        file.flush().await.map_err(|e| DeployError::transfer("upload", e))?;
        file.shutdown().await.map_err(|e| DeployError::transfer("upload", e))?;
        Ok(())
    }

    /// Delete the remote file at `base/rel`.
    async fn remove(&self, base: &str, rel: &str) -> Result<()> {
        self.sftp.remove_file(join(base, rel)).await.map_err(|e| DeployError::transfer("delete", e))?;
        Ok(())
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
            if segment.is_empty() {
                continue;
            }
            let _ = self.sftp.create_dir(prefix.trim_end_matches('/')).await;
        }
    }

    /// Cleanly disconnect; a failed teardown is not worth surfacing.
    async fn close(self) {
        let _ = self.handle.disconnect(Disconnect::ByApplication, "", "").await;
    }
}

/// A `russh` client handler. Accepts the server's host key without checking it
/// against `known_hosts` — the deploy target is named in the project's own
/// config, and this trades strict host-key verification for zero setup, the same
/// posture as `StrictHostKeyChecking=no`. Host-key pinning could be layered on
/// later without touching the rest of the backend.
struct Client;

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _key: &russh::keys::PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// A private current-thread runtime to drive the async SSH exchange.
fn runtime() -> Result<Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| DeployError::transfer("start runtime", e).into())
}

/// Join a base directory and a dist-relative path into a remote path.
fn join(base: &str, rel: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), rel)
}

/// Single-quote a path for the remote shell, escaping embedded quotes.
fn quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}

/// Parse `sha256sum` output (`<hex>  ./path` per line) into digests keyed by the
/// relative path, dropping the `./` prefix `find` emits.
fn parse(output: &str) -> Digests {
    output
        .lines()
        .filter_map(|line| line.split_once("  "))
        .map(|(hash, path)| (path.trim().trim_start_matches("./").to_owned(), hash.to_owned()))
        .filter(|(path, hash)| !path.is_empty() && !hash.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_composes_a_remote_path() {
        assert_eq!(join("/var/www", "posts/a.html"), "/var/www/posts/a.html");
        assert_eq!(join("/var/www/", "a.html"), "/var/www/a.html");
    }

    #[test]
    fn quote_wraps_and_escapes() {
        assert_eq!(quote("/srv/site"), "'/srv/site'");
        assert_eq!(quote("/srv/o'brien"), r"'/srv/o'\''brien'");
    }

    #[test]
    fn parse_reads_sha256sum_output() {
        let out = "\
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  ./index.html
2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae  ./posts/a.css
";
        let digests = parse(out);
        assert_eq!(digests.len(), 2);
        assert_eq!(digests["index.html"], "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(digests["posts/a.css"], "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae");
    }

    #[test]
    fn parse_ignores_blank_and_malformed_lines() {
        assert!(parse("").is_empty());
        assert!(parse("no-double-space ./x").is_empty());
        assert!(parse("\n\n").is_empty());
    }
}
