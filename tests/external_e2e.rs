//! Outbound link checking: `check --external`.
//!
//! Every request goes to a server this test owns, so nothing here depends on
//! the public internet.

mod common;

use std::io::Cursor;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::thread;

use baudelaire::error::BaudelaireErrorKind;

use common::Site;

/// A server that answers `/ok` with 200, `/method` with 405 for HEAD and 200
/// for GET, and everything else with 404. Shuts down when dropped.
struct Host {
    addr: SocketAddr,
    server: Arc<tiny_http::Server>,
}

impl Host {
    fn start() -> Self {
        // Bind an ephemeral port first, so the test never races another for a
        // fixed one.
        let addr = TcpListener::bind("127.0.0.1:0")
            .expect("bind")
            .local_addr()
            .expect("addr");
        let server = Arc::new(tiny_http::Server::http(addr).expect("serve"));
        let worker = Arc::clone(&server);
        thread::spawn(move || {
            for request in worker.incoming_requests() {
                let head = request.method() == &tiny_http::Method::Head;
                let status = match request.url() {
                    "/ok" => 200,
                    // Rejects the method, not the URL: the checker has to ask
                    // again with GET before calling this link dead.
                    "/method" if head => 405,
                    "/method" => 200,
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
        Self { addr, server }
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
        links { external #true }
        "#,
    );
    let links: String = paths
        .iter()
        .map(|path| format!("#link(\"{}\")[link]\n", host.url(path)))
        .collect();
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

/// A host that answers with an error status is the site's problem to fix, so it
/// fails the check and names the page to fix it in.
#[test]
fn a_dead_outbound_link_fails_the_check() {
    let host = Host::start();
    let site = site(&host, &["/ok", "/gone"]);

    let err = site.try_check(|_| {}).expect_err("dead link");
    assert!(matches!(err, BaudelaireErrorKind::DeadLinks(_)), "{err:?}");
    let report = format!("{err}");
    assert!(report.contains("1 dead outbound link"), "{report}");
}

/// A server that refuses HEAD still serves the page; asking once and giving up
/// would report half the web as dead.
#[test]
fn a_head_rejection_is_retried_with_get() {
    let host = Host::start();
    let site = site(&host, &["/method"]);
    site.try_check(|_| {}).expect("check");
}

/// Off by default, and never in a build: a build must produce the same bytes
/// with the network unplugged.
#[test]
fn a_build_never_reaches_the_network() {
    let host = Host::start();
    let site = site(&host, &["/gone"]);
    // `links { external #true }` is set, and the build still succeeds: only
    // `check` acts on it.
    site.stats();
}
