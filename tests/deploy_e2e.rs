//! End-to-end deploy tests: the real backends driven through the public
//! [`baudelaire::deploy::run`], against in-process servers. They exercise the
//! whole round trip (signing, HTTP/SFTP, listing, and the upload/delete plan)
//! without any external service. The digest *skip* path (unchanged files) is
//! covered by unit tests in `deploy`; here we assert the observable effects:
//! new files land, orphans are removed, and a dry run writes nothing.

mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use baudelaire::config::{Config, S3Config};
use baudelaire::deploy;
use baudelaire::error::{BaudelaireErrorKind, DeployError, Result as BResult};
use baudelaire::remote::{Interaction, Options};
use baudelaire::ui::{Level, Ui};

use common::Site;

/// A headless [`Interaction`]: confirms everything, never supplies a secret.
struct Headless;

impl Interaction for Headless {
    fn confirm(&self, _: &str) -> BResult<bool> {
        Ok(true)
    }
    fn secret(&self, _: &str) -> BResult<Option<String>> {
        Ok(None)
    }
}

/// A shared, observable object store keyed by object key.
type Store = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;
/// A shared log of `METHOD path` lines the server received.
type Log = Arc<Mutex<Vec<String>>>;

fn silent() -> Ui {
    Ui::new(Level::Silent)
}

/// Set the AWS credential env vars the S3 backend reads.
#[allow(unsafe_code)]
fn set_aws_creds() {
    // SAFETY: nextest runs each test in its own process, so the environment is
    // unshared and this cannot race another thread.
    unsafe {
        std::env::set_var("AWS_ACCESS_KEY_ID", "AKIDTEST");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "secret");
    }
}

// --- S3 -------------------------------------------------------------------

/// A dist tree with three files, written under `public/`.
fn dist(site: &Site) {
    site.write("public/index.html", "<h1>home</h1>");
    site.write("public/posts/a.html", "post a");
    site.write("public/style.css", "body{}");
}

fn s3_config(site: &Site, port: u16) -> Config {
    let mut config = Config::default();
    config.dist = site.path("public");
    config.deploy.s3 = Some(S3Config {
        bucket: "bucket".into(),
        endpoint: Some(format!("http://127.0.0.1:{port}")),
        region: "us-east-1".into(),
        prefix: String::new(),
        delete: true,
    });
    config
}

/// Minimal S3-compatible mock: ListObjectsV2, PUT, DELETE. Returns its port.
fn spawn_s3(store: Store, log: Log) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            serve_s3(stream, &store, &log);
        }
    });
    port
}

fn serve_s3(mut stream: TcpStream, store: &Store, log: &Log) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = String::new();
    reader.read_line(&mut request).unwrap();
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let path = parts.next().unwrap_or("").to_owned();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header.trim().is_empty() {
            break;
        }
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).unwrap();
    log.lock().unwrap().push(format!("{method} {path}"));

    let body = if path.contains("list-type") {
        listing(&store.lock().unwrap())
    } else {
        let key = object_key(&path);
        match method.as_str() {
            "PUT" => {
                store.lock().unwrap().insert(key, body);
                String::new()
            }
            "DELETE" => {
                store.lock().unwrap().remove(&key);
                String::new()
            }
            _ => String::new(),
        }
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

/// The object key a request path addresses: after the `/bucket/` root, query
/// stripped.
fn object_key(path: &str) -> String {
    let path = path.split('?').next().unwrap_or(path);
    path.trim_start_matches("/bucket/").to_owned()
}

fn listing(store: &BTreeMap<String, Vec<u8>>) -> String {
    let mut xml = String::from("<?xml version=\"1.0\"?><ListBucketResult>");
    for (key, body) in store {
        // The ETag is deliberately bogus: these tests assert on upload/delete,
        // not the unchanged-skip path (which the `plan` unit tests cover), so a
        // non-matching ETag just means every local file re-uploads.
        xml.push_str(&format!(
            "<Contents><Key>{key}</Key><ETag>\"x\"</ETag><Size>{}</Size></Contents>",
            body.len()
        ));
    }
    xml.push_str("</ListBucketResult>");
    xml
}

#[test]
fn s3_deploy_uploads_new_files_and_deletes_orphans() {
    let site = Site::new();
    dist(&site);

    let store: Store = Arc::new(Mutex::new(BTreeMap::new()));
    // An object the build no longer produces: it must be deleted.
    store.lock().unwrap().insert("orphan.html".into(), b"stale".to_vec());
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = spawn_s3(store.clone(), log.clone());

    set_aws_creds();
    let opts = Options { dry_run: false, yes: true, secret: None, interaction: &Headless };
    deploy::run(&s3_config(&site, port), &opts, &silent()).unwrap();

    let store = store.lock().unwrap();
    assert_eq!(store.get("index.html").map(Vec::as_slice), Some(&b"<h1>home</h1>"[..]));
    assert_eq!(store.get("posts/a.html").map(Vec::as_slice), Some(&b"post a"[..]));
    assert!(store.contains_key("style.css"));
    assert!(!store.contains_key("orphan.html"), "orphan should be deleted");
}

#[test]
fn s3_dry_run_lists_but_writes_nothing() {
    let site = Site::new();
    dist(&site);

    let store: Store = Arc::new(Mutex::new(BTreeMap::new()));
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = spawn_s3(store.clone(), log.clone());

    set_aws_creds();
    let opts = Options { dry_run: true, yes: true, secret: None, interaction: &Headless };
    deploy::run(&s3_config(&site, port), &opts, &silent()).unwrap();

    assert!(store.lock().unwrap().is_empty(), "dry run must not upload");
    let log = log.lock().unwrap();
    assert!(log.iter().any(|line| line.contains("list-type")), "it should list the bucket");
    assert!(!log.iter().any(|line| line.starts_with("PUT")), "dry run must not PUT");
}

// --- SSH ------------------------------------------------------------------

use std::collections::HashMap;

use baudelaire::config::SshConfig;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, ChannelOpenHandle, Config as ServerConfig, Handler, Msg, Session, run_stream};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{Handle, Status, StatusCode};

/// The remote base directory the SSH backend mirrors into.
const REMOTE: &str = "/upload";
/// The password the in-process server accepts (matched by the client's secret).
const PASSWORD: &str = "hunter2";

fn ssh_config(site: &Site, port: u16) -> Config {
    let mut config = Config::default();
    config.dist = site.path("public");
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
        rand::RngCore::fill_bytes(&mut rand::rng(), dst);
        Ok(())
    }
}

impl russh::keys::signature::rand_core::TryCryptoRng for Rng {}

/// The SFTP-visible file store the server keeps, keyed by dist-relative path.
type SftpStore = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

fn options<'a>(dry_run: bool, headless: &'a Headless) -> Options<'a> {
    Options { dry_run, yes: true, secret: Some(PASSWORD.into()), interaction: headless }
}

/// Start an in-process SSH server whose `exec` answers with `listing` (the
/// canned `sha256sum` output) and whose SFTP subsystem reads and writes `store`.
/// Returns its port.
fn spawn_ssh(store: SftpStore, listing: String) -> u16 {
    let std_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();
    let port = std_listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let key = PrivateKey::random(&mut Rng, Algorithm::Ed25519).unwrap();
            let config = Arc::new(ServerConfig { keys: vec![key], ..Default::default() });
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            while let Ok((stream, _)) = listener.accept().await {
                let handler = SshServer {
                    store: store.clone(),
                    listing: listing.clone(),
                    channels: HashMap::new(),
                };
                let config = config.clone();
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
        Ok(if password == PASSWORD { Auth::Accept } else { Auth::reject() })
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
                russh_sftp::server::run(channel.into_stream(), Sftp::new(self.store.clone())).await;
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
        path.trim_start_matches(REMOTE).trim_start_matches('/').to_owned()
    }

    fn ok(id: u32) -> Status {
        Status { id, status_code: StatusCode::Ok, error_message: String::new(), language_tag: "en-US".into() }
    }
}

impl russh_sftp::server::Handler for Sftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<russh_sftp::protocol::Name, StatusCode> {
        // Echo the path back as its own canonical form: enough for a client
        // that only ever opens absolute paths.
        Ok(russh_sftp::protocol::Name {
            id,
            files: vec![russh_sftp::protocol::File::dummy(if path == "." { REMOTE } else { &path })],
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

    async fn write(&mut self, id: u32, handle: String, _offset: u64, data: Vec<u8>) -> Result<Status, StatusCode> {
        self.store.lock().unwrap().entry(handle).or_default().extend_from_slice(&data);
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
    store.lock().unwrap().insert("orphan.html".into(), b"stale".to_vec());
    let port = spawn_ssh(store.clone(), orphan_listing("orphan.html"));

    let headless = Headless;
    deploy::run(&ssh_config(&site, port), &options(false, &headless), &silent()).unwrap();

    let store = store.lock().unwrap();
    assert_eq!(store.get("index.html").map(Vec::as_slice), Some(&b"<h1>home</h1>"[..]));
    assert_eq!(store.get("posts/a.html").map(Vec::as_slice), Some(&b"post a"[..]));
    assert!(store.contains_key("style.css"));
    assert!(!store.contains_key("orphan.html"), "orphan should be deleted");
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
    let port = spawn_ssh(store.clone(), String::new());

    // A known_hosts that records a *different* key for this host:port, so the
    // server's ephemeral key reads as changed: the man-in-the-middle guard.
    set_home(&site.root);
    let other = PrivateKey::random(&mut Rng, Algorithm::Ed25519).unwrap();
    site.write(
        ".ssh/known_hosts",
        &format!("[127.0.0.1]:{port} {}\n", other.public_key().to_openssh().unwrap()),
    );

    let mut config = ssh_config(&site, port);
    config.deploy.ssh.as_mut().unwrap().strict = true;
    let headless = Headless;
    let err = deploy::run(&config, &options(false, &headless), &silent()).unwrap_err();

    assert!(
        matches!(err, BaudelaireErrorKind::Deploy(DeployError::HostKeyChanged { .. })),
        "expected a changed-host-key error, got {err:?}"
    );
    assert!(store.lock().unwrap().is_empty(), "a refused host uploads nothing");
}

#[test]
fn ssh_dry_run_writes_nothing() {
    let site = Site::new();
    dist(&site);

    let store: SftpStore = Arc::new(Mutex::new(BTreeMap::new()));
    let port = spawn_ssh(store.clone(), String::new());

    let headless = Headless;
    deploy::run(&ssh_config(&site, port), &options(true, &headless), &silent()).unwrap();

    assert!(store.lock().unwrap().is_empty(), "dry run must not upload");
}
