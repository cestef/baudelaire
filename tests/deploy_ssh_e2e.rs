//! End-to-end SSH deploy tests: the real `russh`-backed backend driven through
//! the public [`baudelaire::deploy::run`], against an in-process SSH/SFTP
//! server. Split out from `deploy_e2e.rs` (which keeps the S3 half) because the
//! backend under test is compiled in only by the `ssh` feature, and a whole file
//! is the one granularity cargo will skip building outright.
#![cfg(feature = "ssh")]

mod common;

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use baudelaire::config::Config;
use baudelaire::deploy;
use baudelaire::error::{BaudelaireErrorKind, DeployError, Result as BResult};
use baudelaire::remote::{Interaction, Options};

use common::{Site, dist, silent};

/// A headless [`Interaction`]: confirms everything, never supplies a secret.
struct Headless;

impl Interaction for Headless {
    fn interactive(&self) -> bool {
        true
    }
    fn confirm(&self, _: &str) -> BResult<bool> {
        Ok(true)
    }
    fn secret(&self, _: &str) -> BResult<Option<String>> {
        Ok(None)
    }
}

use std::collections::HashMap;

use baudelaire::config::SshConfig;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{
    Auth, ChannelOpenHandle, Config as ServerConfig, Handler, Msg, Session, run_stream,
};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{Handle, Status, StatusCode};

/// The remote base directory the SSH backend mirrors into.
const REMOTE: &str = "/upload";
/// The password the in-process server accepts (matched by the client's secret).
const PASSWORD: &str = "hunter2";

fn ssh_config(site: &Site, port: u16) -> Config {
    let mut config = Config::default();
    config.paths.dist = site.path("public");
    config.deploy.ssh = Some(SshConfig {
        host: "127.0.0.1".into(),
        path: REMOTE.into(),
        port,
        user: Some("deploy".into()),
        key: None,
        // Never touch the developer's ~/.ssh/known_hosts from a test.
        strict: false,
        delete: true,
    });
    config
}

/// A random source implementing the exact `rand_core` `ssh-key` pins (which
/// differs from the `rand` crate's own), backed by `rand` for entropy. In
/// rand_core 0.10 the fallible `TryRng`/`TryCryptoRng` are the base traits;
/// with `Error = Infallible` the infallible `Rng`/`CryptoRng` follow by blanket.
struct Rng;

impl russh::keys::signature::rand_core::TryRng for Rng {
    type Error = std::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(rand::random())
    }
    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(rand::random())
    }
    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        dst.fill_with(rand::random);
        Ok(())
    }
}

impl russh::keys::signature::rand_core::TryCryptoRng for Rng {}

/// The SFTP-visible file store the server keeps, keyed by dist-relative path.
type SftpStore = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

fn options(dry_run: bool, headless: &Headless) -> Options<'_> {
    Options {
        dry_run,
        yes: true,
        secret: Some(PASSWORD.into()),
        interaction: headless,
    }
}

/// Start an in-process SSH server whose `exec` answers with `listing` (the
/// canned `sha256sum` output) and whose SFTP subsystem reads and writes `store`.
/// Returns its port.
fn spawn_ssh(store: SftpStore, listing: String) -> u16 {
    let std_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();
    let port = std_listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let key = PrivateKey::random(&mut Rng, Algorithm::Ed25519).unwrap();
            let config = Arc::new(ServerConfig {
                keys: vec![key],
                ..Default::default()
            });
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            while let Ok((stream, _)) = listener.accept().await {
                let handler = SshServer {
                    store: Arc::clone(&store),
                    listing: listing.clone(),
                    channels: HashMap::new(),
                };
                let config = Arc::clone(&config);
                tokio::spawn(async move {
                    if let Ok(session) = run_stream(config, stream, handler).await {
                        let _ = session.await;
                    }
                });
            }
        });
    });
    port
}

/// The SSH server: accepts a password, streams canned `exec` output, and hands
/// the sftp subsystem an in-memory file store.
struct SshServer {
    store: SftpStore,
    listing: String,
    channels: HashMap<ChannelId, Channel<Msg>>,
}

impl Handler for SshServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if password == PASSWORD {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        // Force the client onto password auth, ignoring any ssh-agent keys.
        Ok(Auth::reject())
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        // Dropping the handle without accepting sends AdministrativelyProhibited.
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, self.listing.clone().into_bytes())?;
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            session.channel_success(channel)?;
            if let Some(channel) = self.channels.remove(&channel) {
                russh_sftp::server::run(channel.into_stream(), Sftp::new(Arc::clone(&self.store)))
                    .await;
            }
        } else {
            session.channel_failure(channel)?;
        }
        Ok(())
    }
}

/// An in-memory SFTP backend: create/write/close upload a file, remove deletes
/// one, mkdir is a no-op. Paths are stored dist-relative (the `/upload` base
/// stripped), matching the object keys the tests assert on.
struct Sftp {
    store: SftpStore,
}

impl Sftp {
    fn new(store: SftpStore) -> Self {
        Self { store }
    }

    fn rel(path: &str) -> String {
        path.trim_start_matches(REMOTE)
            .trim_start_matches('/')
            .to_owned()
    }

    fn ok(id: u32) -> Status {
        Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: "en-US".into(),
        }
    }
}

impl russh_sftp::server::Handler for Sftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }

    async fn realpath(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Name, StatusCode> {
        // Echo the path back as its own canonical form: enough for a client
        // that only ever opens absolute paths.
        Ok(russh_sftp::protocol::Name {
            id,
            files: vec![russh_sftp::protocol::File::dummy(if path == "." {
                REMOTE
            } else {
                &path
            })],
        })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        _pflags: russh_sftp::protocol::OpenFlags,
        _attrs: russh_sftp::protocol::FileAttributes,
    ) -> Result<Handle, StatusCode> {
        let rel = Self::rel(&filename);
        self.store.lock().unwrap().insert(rel.clone(), Vec::new());
        Ok(Handle { id, handle: rel })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        _offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        self.store
            .lock()
            .unwrap()
            .entry(handle)
            .or_default()
            .extend_from_slice(&data);
        Ok(Self::ok(id))
    }

    async fn close(&mut self, id: u32, _handle: String) -> Result<Status, StatusCode> {
        Ok(Self::ok(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        _path: String,
        _attrs: russh_sftp::protocol::FileAttributes,
    ) -> Result<Status, StatusCode> {
        Ok(Self::ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, StatusCode> {
        self.store.lock().unwrap().remove(&Self::rel(&filename));
        Ok(Self::ok(id))
    }
}

/// A `sha256sum` line for a remote file the build no longer produces.
fn orphan_listing(rel: &str) -> String {
    format!("{}  ./{rel}\n", "0".repeat(64))
}

#[test]
fn ssh_deploy_uploads_new_files_and_deletes_orphans() {
    let site = Site::new();
    dist(&site);

    let store: SftpStore = Arc::new(Mutex::new(BTreeMap::new()));
    store
        .lock()
        .unwrap()
        .insert("orphan.html".into(), b"stale".to_vec());
    let port = spawn_ssh(Arc::clone(&store), orphan_listing("orphan.html"));

    let headless = Headless;
    deploy::run(
        &ssh_config(&site, port),
        &options(false, &headless),
        &silent(),
    )
    .unwrap();

    let store = store.lock().unwrap();
    assert_eq!(
        store.get("index.html").map(Vec::as_slice),
        Some(&b"<h1>home</h1>"[..])
    );
    assert_eq!(
        store.get("posts/a.html").map(Vec::as_slice),
        Some(&b"post a"[..])
    );
    assert!(store.contains_key("style.css"));
    assert!(
        !store.contains_key("orphan.html"),
        "orphan should be deleted"
    );
}

/// Point `$HOME` at `dir` so the ssh backend reads `dir/.ssh/known_hosts`.
#[allow(unsafe_code)]
fn set_home(dir: &std::path::Path) {
    // SAFETY: nextest runs each test in its own process, so the environment is
    // unshared and this cannot race another thread.
    unsafe { std::env::set_var("HOME", dir) }
}

#[test]
fn ssh_refuses_a_changed_host_key() {
    let site = Site::new();
    dist(&site);
    let store: SftpStore = Arc::new(Mutex::new(BTreeMap::new()));
    let port = spawn_ssh(Arc::clone(&store), String::new());

    // A known_hosts that records a *different* key for this host:port, so the
    // server's ephemeral key reads as changed: the man-in-the-middle guard.
    set_home(&site.root);
    let other = PrivateKey::random(&mut Rng, Algorithm::Ed25519).unwrap();
    site.write(
        ".ssh/known_hosts",
        &format!(
            "[127.0.0.1]:{port} {}\n",
            other.public_key().to_openssh().unwrap()
        ),
    );

    let mut config = ssh_config(&site, port);
    config.deploy.ssh.as_mut().unwrap().strict = true;
    let headless = Headless;
    let err = deploy::run(&config, &options(false, &headless), &silent()).unwrap_err();

    assert!(
        matches!(
            err,
            BaudelaireErrorKind::Deploy(DeployError::HostKeyChanged { .. })
        ),
        "expected a changed-host-key error, got {err:?}"
    );
    assert!(
        store.lock().unwrap().is_empty(),
        "a refused host uploads nothing"
    );
}

#[test]
fn ssh_dry_run_writes_nothing() {
    let site = Site::new();
    dist(&site);

    let store: SftpStore = Arc::new(Mutex::new(BTreeMap::new()));
    let port = spawn_ssh(Arc::clone(&store), String::new());

    let headless = Headless;
    deploy::run(
        &ssh_config(&site, port),
        &options(true, &headless),
        &silent(),
    )
    .unwrap();

    assert!(store.lock().unwrap().is_empty(), "dry run must not upload");
}
