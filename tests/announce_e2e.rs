//! (Compiled only with the `announce` feature, which owns everything below.)
#![cfg(feature = "announce")]

//! End-to-end announce tests: the standard.site backend driven through the
//! public [`baudelaire::announce::run`], against an in-process PDS.
//!
//! The build-time half of announcing (the verification artifacts) is covered by
//! `scenarios/announce.kdl`, which deliberately touches no network. This is the
//! other half, and the half that matters most: announcing is the one thing
//! baudelaire does that makes authenticated, *destructive* writes to somebody
//! else's account. The reconcile loop deletes every remote record no longer
//! backed by a page, and until this file existed nothing exercised that path,
//! nor session login, nor the XRPC client at all.

mod common;

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use baudelaire::announce;
use baudelaire::config::{Config, StandardConfig};
use baudelaire::error::Result as BResult;
use baudelaire::remote::{Interaction, Options};

use common::{Site, silent};

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

/// The DID the fake PDS authenticates as.
const DID: &str = "did:plc:test";
/// The document collection the reconcile loop diffs.
const DOCUMENTS: &str = "site.standard.document";

/// The record keys currently in the repo, keyed `collection/rkey`.
type Records = Arc<Mutex<BTreeSet<String>>>;
/// A shared log of the XRPC method names the server was called with.
type Log = Arc<Mutex<Vec<String>>>;

/// Set the app password the backend reads, so no prompt is reached.
#[allow(unsafe_code)]
fn set_password() {
    // SAFETY: nextest runs each test in its own process, so the environment is
    // unshared and this cannot race another thread.
    unsafe {
        std::env::set_var("BAUDELAIRE_ATPROTO_PASSWORD", "app-password");
    }
}

/// Minimal PDS: the five XRPC methods an announce actually calls. Returns its
/// port.
fn spawn_pds(records: Records, log: Log) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            serve(stream, &records, &log);
        }
    });
    port
}

fn serve(mut stream: TcpStream, records: &Records, log: &Log) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = String::new();
    reader.read_line(&mut request).unwrap();
    let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();

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

    // `/xrpc/<nsid>?query` -> `<nsid>`.
    let nsid = path
        .split('?')
        .next()
        .unwrap_or("")
        .trim_start_matches("/xrpc/")
        .to_owned();
    log.lock().unwrap().push(nsid.clone());
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();

    let response = match nsid.as_str() {
        "com.atproto.server.createSession" => {
            serde_json::json!({ "did": DID, "accessJwt": "token" })
        }
        "com.atproto.identity.resolveHandle" => serde_json::json!({ "did": DID }),
        "com.atproto.repo.listRecords" => {
            let collection = query(&path, "collection");
            let uris: Vec<serde_json::Value> = records
                .lock()
                .unwrap()
                .iter()
                .filter_map(|key| key.strip_prefix(&format!("{collection}/")))
                .map(|rkey| serde_json::json!({ "uri": format!("at://{DID}/{collection}/{rkey}") }))
                .collect();
            // No `cursor`: one page, which ends the walk.
            serde_json::json!({ "records": uris })
        }
        "com.atproto.repo.putRecord" => {
            records.lock().unwrap().insert(record_key(&body));
            serde_json::json!({})
        }
        "com.atproto.repo.deleteRecord" => {
            records.lock().unwrap().remove(&record_key(&body));
            serde_json::json!({})
        }
        _ => serde_json::json!({}),
    }
    .to_string();

    let reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len()
    );
    stream.write_all(reply.as_bytes()).unwrap();
    stream.flush().unwrap();
}

/// The `collection/rkey` a put/delete body addresses.
fn record_key(body: &serde_json::Value) -> String {
    let field = |key: &str| body.get(key).and_then(|v| v.as_str()).unwrap_or_default();
    format!("{}/{}", field("collection"), field("rkey"))
}

/// One query parameter of a request path.
fn query(path: &str, key: &str) -> String {
    path.split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("")
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .unwrap_or_default()
        .to_owned()
}

/// A site with two dated pages and one undated. Changes into it, which is where
/// the skip-cache and the content tree both resolve from.
fn site() -> Site {
    let site = Site::with(common::CONFIG);
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\", date: datetime(year: 2026, month: 1, day: 2),)\nbody\n",
    );
    site.write(
        "content/posts/second.typ",
        "#let frontmatter = (title: \"Second\", date: datetime(year: 2026, month: 1, day: 3),)\nbody\n",
    );
    // Undated pages are not documents: standard.site requires a `publishedAt`.
    site.write(
        "content/posts/undated.typ",
        "#let frontmatter = (title: \"Undated\",)\nbody\n",
    );
    std::env::set_current_dir(&site.root).unwrap();
    site
}

fn config_at(site: &Site, port: u16) -> Config {
    let mut config = Config::default();
    config.root = site.root.clone();
    config.url = Some("https://example.com".into());
    config.site = Some("T".into());
    config.announce.standard = Some(StandardConfig {
        handle: "me.example".into(),
        did: Some(DID.into()),
        pds: format!("http://127.0.0.1:{port}"),
        discover: true,
        icon: None,
        verify: Default::default(),
    });
    config
}

fn options(dry_run: bool) -> Options<'static> {
    Options {
        dry_run,
        yes: true,
        secret: None,
        interaction: &Headless,
    }
}

/// The destructive path, and the reason this file exists: a record whose page is
/// gone must be deleted from the remote repository. The remote is the source of
/// truth, so a stale record is not merely left behind, it is removed.
#[test]
fn a_record_whose_page_is_gone_is_deleted_from_the_repo() {
    let records: Records = Arc::new(Mutex::new(BTreeSet::new()));
    // A record for a page this site no longer has.
    records
        .lock()
        .unwrap()
        .insert(format!("{DOCUMENTS}/staleaaaaaaaa"));
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = spawn_pds(records.clone(), log.clone());

    let site = site();
    set_password();
    announce::run(&config_at(&site, port), &options(false), &silent()).unwrap();

    let records = records.lock().unwrap();
    assert!(
        !records.contains(&format!("{DOCUMENTS}/staleaaaaaaaa")),
        "the orphaned record should have been deleted: {records:?}"
    );
    // Both dated pages were published, under their derived keys...
    assert!(records.contains(&format!("{DOCUMENTS}/{}", rkey("/posts/hello/"))));
    assert!(records.contains(&format!("{DOCUMENTS}/{}", rkey("/posts/second/"))));
    // ...the undated one was not, and the publication record went up.
    assert_eq!(
        records.iter().filter(|k| k.starts_with(DOCUMENTS)).count(),
        2,
        "only the dated pages are documents: {records:?}"
    );
    assert!(records.contains("site.standard.publication/self"));

    let log = log.lock().unwrap();
    assert!(log.contains(&"com.atproto.server.createSession".to_owned()));
    assert!(log.contains(&"com.atproto.repo.deleteRecord".to_owned()));
}

/// A dry run diffs against the live repo, so it authenticates nothing and writes
/// nothing: no session, no puts, and above all no deletes.
#[test]
fn a_dry_run_diffs_the_live_repo_without_writing() {
    let records: Records = Arc::new(Mutex::new(BTreeSet::new()));
    records
        .lock()
        .unwrap()
        .insert(format!("{DOCUMENTS}/staleaaaaaaaa"));
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = spawn_pds(records.clone(), log.clone());

    let site = site();
    set_password();
    announce::run(&config_at(&site, port), &options(true), &silent()).unwrap();

    assert_eq!(
        *records.lock().unwrap(),
        BTreeSet::from([format!("{DOCUMENTS}/staleaaaaaaaa")]),
        "a dry run must leave the repo exactly as it found it"
    );
    let log = log.lock().unwrap();
    assert!(
        log.contains(&"com.atproto.repo.listRecords".to_owned()),
        "it should still read the live repo to build the plan"
    );
    assert!(
        !log.contains(&"com.atproto.server.createSession".to_owned()),
        "a preview needs no credentials: reads are public XRPC"
    );
    assert!(
        !log.iter().any(|nsid| nsid.ends_with("deleteRecord")),
        "a dry run must not delete"
    );
}

/// A `did` pinned in config that disagrees with the account actually
/// authenticated is fatal: the build emitted verification artifacts naming one
/// identity while the records would land under another.
#[test]
fn a_mismatched_did_pin_refuses_to_publish() {
    let records: Records = Arc::new(Mutex::new(BTreeSet::new()));
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let port = spawn_pds(records.clone(), log.clone());

    let site = site();
    set_password();
    let mut config = config_at(&site, port);
    config.announce.standard.as_mut().unwrap().did = Some("did:plc:somebodyelse".into());

    let err = announce::run(&config, &options(false), &silent()).expect_err("should refuse");
    assert!(format!("{err}").contains("did"), "{err}");
    assert!(
        records.lock().unwrap().is_empty(),
        "nothing may be written once the identity is in doubt"
    );
}

/// The record key the backend derives for a page path.
fn rkey(path: &str) -> String {
    baudelaire::atproto::Rkey::derived(path).as_str().to_owned()
}
