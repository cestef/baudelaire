//! End-to-end deploy tests: the real backends driven through the public
//! [`baudelaire::deploy::Deploy::run`], against in-process servers. They exercise the
//! whole round trip (signing, HTTP/SFTP, listing, and the upload/delete plan)
//! without any external service. The digest *skip* path (unchanged files) is
//! covered by unit tests in `deploy`; here we assert the observable effects:
//! new files land, orphans are removed, and a dry run writes nothing.

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use baudelaire::config::{CacheControl, Config, S3Config};
use baudelaire::deploy;
use baudelaire::error::Result as BResult;
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

/// A shared, observable object store keyed by object key.
type Store = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;
/// A shared log of `METHOD path` lines the server received.
type Log = Arc<Mutex<Vec<String>>>;

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
fn s3_config(site: &Site, port: u16) -> Config {
    let mut config = Config::default();
    config.paths.dist = site.path("public");
    config.assets.fingerprint = true;
    config.deploy.s3 = Some(S3Config {
        bucket: "bucket".into(),
        endpoint: Some(format!("http://127.0.0.1:{port}")),
        region: None,
        prefix: String::new(),
        delete: true,
    });
    config.caching = CacheControl {
        enabled: true,
        immutable: "public, max-age=31536000, immutable".into(),
        default: "public, max-age=0, must-revalidate".into(),
    };
    config
}

/// Minimal S3-compatible mock: ListObjectsV2, PUT, DELETE. Returns its port.
fn spawn_s3(store: Store, log: Log) -> u16 {
    spawn_s3_answering(store, log, None)
}

/// `refusing` makes every write answer with that S3 error code, in the XML
/// shape a real bucket uses, so a test can assert on what the *host* said
/// rather than on the status alone.
fn spawn_s3_answering(store: Store, log: Log, refusing: Option<&'static str>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            serve_s3(stream, &store, &log, refusing);
        }
    });
    port
}

fn serve_s3(mut stream: TcpStream, store: &Store, log: &Log, refusing: Option<&'static str>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = String::new();
    reader.read_line(&mut request).unwrap();
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let path = parts.next().unwrap_or("").to_owned();

    let mut length = 0usize;
    let mut cache_control: Option<String> = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header.trim().is_empty() {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        if let Some(value) = lower.strip_prefix("cache-control:") {
            cache_control = Some(value.trim().to_owned());
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).unwrap();
    log.lock().unwrap().push(match cache_control {
        Some(cache) => format!("{method} {path} [{cache}]"),
        None => format!("{method} {path}"),
    });

    if let Some(code) = refusing.filter(|_| method != "GET") {
        let body = format!(
            "<?xml version=\"1.0\"?><Error><Code>{code}</Code>\
             <Message>The request signature we calculated does not match</Message></Error>"
        );
        let response = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
        return;
    }
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
        write!(
            xml,
            "<Contents><Key>{key}</Key><ETag>\"x\"</ETag><Size>{}</Size></Contents>",
            body.len()
        )
        .unwrap();
    }
    xml.push_str("</ListBucketResult>");
    xml
}

#[test]
fn s3_deploy_uploads_new_files_and_deletes_orphans() {
    let site = Site::new();
    dist(&site);
    // A content-addressed asset, the one kind of file that may be cached
    // forever: its name changes whenever its bytes do.
    site.write("public/assets/app.abc123def456.css", "body{}");

    let store: Store = Arc::new(Mutex::new(BTreeMap::new()));
    // An object the build no longer produces: it must be deleted.
    store
        .lock()
        .unwrap()
        .insert("orphan.html".into(), b"stale".to_vec());
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = spawn_s3(Arc::clone(&store), Arc::clone(&log));

    set_aws_creds();
    let opts = Options {
        dry_run: false,
        yes: true,
        secret: None,
        interaction: &Headless,
    };
    deploy::Deploy::run(&s3_config(&site, port), &opts, &silent()).unwrap();

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

    // `assets { fingerprint }` renames a file after its own content, which is
    // what makes it safe to cache forever. A raw bucket sets no `Cache-Control`
    // of its own, so without this the whole point of hashing is lost at the
    // last step.
    let log = log.lock().unwrap();
    let line = |key: &str| {
        log.iter()
            .find(|l| l.starts_with("PUT") && l.contains(key))
            .unwrap_or_else(|| panic!("no PUT for {key} in {log:?}"))
            .clone()
    };
    assert!(
        line("style.css").contains("[public, max-age=0, must-revalidate]"),
        "an unhashed name keeps its name across builds: {:?}",
        line("style.css")
    );
    assert!(
        line("index.html").contains("[public, max-age=0, must-revalidate]"),
        "a page is never immutable: {:?}",
        line("index.html")
    );
    assert!(
        line("app.abc123def456.css").contains("[public, max-age=31536000, immutable]"),
        "a hashed asset should be cached forever: {:?}",
        line("app.abc123def456.css")
    );
}

/// A bucket that refuses a write says why, and the deploy says what it said.
///
/// The agent used to surface a non-2xx as a transport error, so the branch that
/// reads the bucket's own `<Error><Code>` body was unreachable and every refusal
/// reported a bare status: a wrong secret, a missing bucket and a rate limit
/// were indistinguishable.
#[test]
fn s3_deploy_reports_the_reason_the_bucket_gave() {
    let site = Site::new();
    dist(&site);

    let store: Store = Arc::new(Mutex::new(BTreeMap::new()));
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = spawn_s3_answering(
        Arc::clone(&store),
        Arc::clone(&log),
        Some("SignatureDoesNotMatch"),
    );

    set_aws_creds();
    let opts = Options {
        dry_run: false,
        yes: true,
        secret: None,
        interaction: &Headless,
    };
    let err = deploy::Deploy::run(&s3_config(&site, port), &opts, &silent()).expect_err("refused");
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(rendered.contains("SignatureDoesNotMatch"), "{rendered}");
    assert!(rendered.contains("403"), "{rendered}");
}

#[test]
fn s3_dry_run_lists_but_writes_nothing() {
    let site = Site::new();
    dist(&site);

    let store: Store = Arc::new(Mutex::new(BTreeMap::new()));
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = spawn_s3(Arc::clone(&store), Arc::clone(&log));

    set_aws_creds();
    let opts = Options {
        dry_run: true,
        yes: true,
        secret: None,
        interaction: &Headless,
    };
    deploy::Deploy::run(&s3_config(&site, port), &opts, &silent()).unwrap();

    assert!(store.lock().unwrap().is_empty(), "dry run must not upload");
    let log = log.lock().unwrap();
    assert!(
        log.iter().any(|line| line.contains("list-type")),
        "it should list the bucket"
    );
    assert!(
        !log.iter().any(|line| line.starts_with("PUT")),
        "dry run must not PUT"
    );
}
