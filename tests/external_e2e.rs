//! Outbound link checking: `check --external`.
//!
//! Every request goes to a server this test owns, never the public internet.

mod common;

use std::fmt::Write as _;
use std::io::Cursor;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use baudelaire::error::BaudelaireErrorKind;

use common::Site;

/// A server that answers `/ok` with 200, `/method` with 405 for HEAD and 200
/// for GET, and everything else with 404. `/ok` starts answering 404 once
/// [`Host::breaks`] is called. Shuts down when dropped.
struct Host {
    addr: SocketAddr,
    server: Arc<tiny_http::Server>,
    broken: Arc<AtomicBool>,
}

impl Host {
    fn start() -> Self {
        let addr = TcpListener::bind("127.0.0.1:0")
            .expect("bind")
            .local_addr()
            .expect("addr");
        let server = Arc::new(tiny_http::Server::http(addr).expect("serve"));
        let worker = Arc::clone(&server);
        let broken = Arc::new(AtomicBool::new(false));
        let flips = Arc::clone(&broken);
        thread::spawn(move || {
            for request in worker.incoming_requests() {
                let head = request.method() == &tiny_http::Method::Head;
                let status = match request.url() {
                    // Rejects the method, not the URL.
                    "/method" if head => 405,
                    "/ok" if flips.load(Ordering::SeqCst) => 404,
                    "/ok" | "/method" => 200,
                    _ => 404,
                };
                let response = tiny_http::Response::new(
                    tiny_http::StatusCode(status),
                    Vec::new(),
                    Cursor::new(Vec::new()),
                    Some(0),
                    None,
                );
                let _ = request.respond(response);
            }
        });
        Self {
            addr,
            server,
            broken,
        }
    }

    /// Every later request for `/ok` answers 404, as a link that rots between
    /// two runs does.
    fn breaks(&self) {
        self.broken.store(true, Ordering::SeqCst);
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

/// A site whose one page links to each of `paths` on `host`.
fn site(host: &Host, paths: &[&str]) -> Site {
    let site = Site::with(
        r#"
        site "T"
        paths { content "content"; dist "public" }
        check { external { fresh "0s" } }
        "#,
    );
    let mut links = String::new();
    for path in paths {
        writeln!(links, "#link(\"{}\")[link]", host.url(path)).unwrap();
    }
    site.write(
        "content/index.typ",
        &format!("#let frontmatter = (title: \"Home\",)\n{links}"),
    );
    site
}

#[test]
fn a_live_outbound_link_passes() {
    let host = Host::start();
    let site = site(&host, &["/ok"]);
    site.try_check(|_| {}).expect("check");
}

#[test]
fn a_dead_outbound_link_fails_the_check() {
    let host = Host::start();
    let site = site(&host, &["/ok", "/gone"]);

    let err = site.try_check(|_| {}).expect_err("dead link");
    assert!(matches!(err, BaudelaireErrorKind::DeadLinks(_)), "{err:?}");
    let report = format!("{err}");
    assert!(report.contains("1 dead outbound link"), "{report}");
}

/// The check is incremental, and a cache hit used to replay no outbound links
/// at all: a CI gate stopped gating the moment its cache was warm.
#[test]
fn a_second_check_still_probes_a_cached_page() {
    let host = Host::start();
    let site = site(&host, &["/ok"]);
    site.try_check(|_| {}).expect("first check");

    host.breaks();
    let err = site.try_check(|_| {}).expect_err("the link rotted");
    assert!(matches!(err, BaudelaireErrorKind::DeadLinks(_)), "{err:?}");
}

#[test]
fn a_head_rejection_is_retried_with_get() {
    let host = Host::start();
    let site = site(&host, &["/method"]);
    site.try_check(|_| {}).expect("check");
}

#[test]
fn a_build_never_reaches_the_network() {
    let host = Host::start();
    let site = site(&host, &["/gone"]);
    // `check { external #true }` is set, and only `check` acts on it.
    site.stats();
}
