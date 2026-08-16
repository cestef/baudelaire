//! Answering a request: which file a URL names, and refusing the ones outside.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use tiny_http::{Header, Request, Response, Server};

use super::live::Live;
use super::open::{Open, Unopenable};
use crate::config::{Config, Percent};
use crate::error::Result;
use crate::mime::Mime;
use crate::ui::{Level, Ui};

/// Serves files from `dist`, optionally injecting live reload. Moved into the
/// request-handling thread, so it is `Send` and self-contained.
pub(super) struct Handler {
    /// Shared with the rebuild loop, so a `config.kdl` reload that moves `dist`
    /// or changes `url` reaches the request thread.
    route: Arc<Mutex<Route>>,
    live: Option<Live>,
    /// The handler's own [`Ui`] at the session's verbosity, so per-request
    /// logging honors `--quiet` without sharing the rebuild loop's writer.
    ui: Ui,
}

/// What the server reads from, what URL prefix it strips, and how it opens a
/// source file: the config-derived values a reload can move out from under a
/// running handler.
#[derive(Clone)]
pub(super) struct Route {
    /// The served root, canonical so every per-request traversal check compares
    /// canonical paths (with `..` and symlinks resolved) against it.
    dist: PathBuf,
    /// The path the site is served under, stripped from each request so a
    /// subdirectory-hosted site (`url "https://host/docs"`) previews locally.
    base: String,
    /// The editor a source-mapped preview hands a location to, absent until
    /// `serve { editor .. }` names one.
    open: Option<Open>,
    /// The declared language codes, so an unmatched URL under `/fr/` is answered
    /// with the French not-found page. Empty on a single-language site.
    langs: Vec<String>,
}

impl Route {
    /// Resolve a URL path to a file under [`dist`](Route::dist), honoring clean
    /// URLs. Every candidate is checked to stay within it (see
    /// [`within`](Route::within)), so a `..`-laden or symlinked request can
    /// never escape the served root.
    fn resolve(&self, url: &str) -> Option<PathBuf> {
        let path = url.split('?').next().unwrap_or(url);
        let rel = path
            .strip_prefix(&self.base)
            .unwrap_or(path)
            .trim_start_matches('/');
        let rel = Percent::decode(rel);
        let base = self.dist.join(&rel);
        self.within(&base)
            .or_else(|| self.within(&base.join(Config::INDEX)))
            .or_else(|| self.within(&self.dist.join(format!("{rel}.html"))))
    }

    /// The canonical path of `candidate` when it is an existing file inside
    /// `dist`, else `None`. The single guard against path traversal: both the
    /// root and the candidate are canonical, so `..` segments and symlinks that
    /// would leave the served tree are rejected before any read.
    fn within(&self, candidate: &Path) -> Option<PathBuf> {
        let canon = crate::fs::canonicalize(candidate).ok()?;
        (canon.starts_with(&self.dist) && canon.is_file()).then_some(canon)
    }

    pub(super) fn new(config: &Config) -> Self {
        let dist = config.paths.dist.clone();
        Self {
            dist: crate::fs::canonicalize(&dist).unwrap_or(dist),
            base: config.base_path().to_owned(),
            open: Open::new(config),
            langs: if config.multilingual() {
                config.langs().iter().map(|c| (*c).to_owned()).collect()
            } else {
                Vec::new()
            },
        }
    }
}

impl Route {
    /// The declared language a request URL sits under, if any: the first path
    /// segment, when it names one.
    fn scope(&self, url: &str) -> Option<&str> {
        let path = url.strip_prefix(&self.base).unwrap_or(url);
        let head = path.trim_start_matches('/').split('/').next()?;
        self.langs
            .iter()
            .find(|code| *code == head)
            .map(String::as_str)
    }
}

impl Handler {
    pub(super) fn new(route: Arc<Mutex<Route>>, live: Option<Live>, level: Level) -> Self {
        Self {
            route,
            live,
            ui: Ui::new(level),
        }
    }

    /// Run the request loop on its own thread (used while watching, so the main
    /// thread is free for the rebuild loop).
    pub(super) fn spawn(self, server: Server) {
        std::thread::spawn(move || self.serve(&server));
    }

    /// Blocking request loop.
    pub(super) fn serve(&self, server: &Server) {
        while let Ok(req) = server.recv() {
            self.handle(req);
        }
    }

    /// Serve the live-reload stream, open a source location, or map the URL to
    /// a file under `dist`. The resolved file is bound before the match, since
    /// a guard held into an arm would deadlock against `respond_404`.
    fn handle(&self, req: Request) {
        let url = req.url().to_owned();
        if let Some(live) = &self.live
            && url.starts_with(Live::ENDPOINT)
        {
            live.serve(req);
            return;
        }
        if self.live.is_some() && url.starts_with(Open::ENDPOINT) {
            self.open(req, &url);
            return;
        }
        let file = self.route.lock().resolve(&url);
        match file {
            Some(file) => self.serve_file(req, &file, 200),
            None => self.respond_404(req, &url),
        }
    }

    /// Serve a file under `status` with content-type guessing, injecting the
    /// live-reload client into HTML when live reload is enabled.
    fn serve_file(&self, req: Request, path: &Path, status: u16) {
        let mime = Mime::of(path);
        let Ok(mut body) = crate::fs::read(path) else {
            self.ui.request(500, &path.display().to_string());
            let _ = req.respond(Response::empty(500));
            return;
        };
        if self.live.is_some() && mime.html() {
            body.extend_from_slice(Live::SCRIPT.as_bytes());
        }
        let mut response = Response::from_data(body).with_status_code(status);
        if let Ok(header) = Header::from_bytes(b"Content-Type", mime.header().as_bytes()) {
            response = response.with_header(header);
        }
        let _ = req.respond(response);
    }

    /// Respond with the site's own not-found page when it emits one (the same
    /// file a static host serves for unmatched URLs), else an empty 404.
    fn respond_404(&self, req: Request, url: &str) {
        self.ui.request(404, url);
        let found = {
            let route = self.route.lock();
            let scoped = route
                .scope(url)
                .map(|code| route.dist.join(code).join(crate::config::Config::NOT_FOUND));
            scoped
                .into_iter()
                .chain([route.dist.join(crate::config::Config::NOT_FOUND)])
                .find_map(|candidate| route.within(&candidate))
        };
        match found {
            Some(page) => self.serve_file(req, &page, 404),
            None => {
                let _ = req.respond(Response::empty(404));
            }
        }
    }

    /// Hand a stamped source location to the configured editor, checked as any
    /// request is: same-origin, parseable, and naming one of the project's own
    /// files.
    fn open(&self, req: Request, url: &str) {
        let outcome = self.launch(&req, url);
        let status = outcome.as_ref().err().map_or(200, Unopenable::status);
        self.ui.request(status, url);
        let body = outcome.err().map(|why| why.body()).unwrap_or_default();
        let _ = req.respond(Response::from_string(body).with_status_code(status));
    }

    fn launch(&self, req: &Request, url: &str) -> Result<(), Unopenable> {
        if !Self::same_origin(req) {
            return Err(Unopenable::Foreign);
        }
        let raw = Self::addressed(url).ok_or(Unopenable::Unaddressed)?;
        let decoded = Percent::decode(raw);
        let at = At::parse(&decoded).ok_or_else(|| Unopenable::Malformed(decoded.clone()))?;
        let open = self.route.lock().open.clone();
        open.ok_or(Unopenable::Unconfigured)?.at(&at)
    }

    /// Whether the request came from the page this server served. A client that
    /// sends no `Sec-Fetch-Site` at all (curl, a test) is allowed through.
    fn same_origin(req: &Request) -> bool {
        req.headers()
            .iter()
            .find(|header| header.field.equiv("Sec-Fetch-Site"))
            .is_none_or(|header| header.value.as_str() == "same-origin")
    }

    /// The source location a request names, still encoded. An absent `at` and an
    /// empty one both answer `None`, becoming [`Unopenable::Unaddressed`].
    fn addressed(url: &str) -> Option<&str> {
        Self::query(url, "at").filter(|raw| !raw.is_empty())
    }

    /// The value of `key` in a URL's query string, undecoded.
    fn query<'a>(url: &'a str, key: &str) -> Option<&'a str> {
        url.split_once('?')?
            .1
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find_map(|(k, value)| (k == key).then_some(value))
    }
}

/// A source location as a stamped element spells it: `file:line:column`, the
/// value [`crate::render`] writes into `data-typst`.
pub(super) struct At<'a> {
    /// Project-relative, as the compiler named the file.
    pub(super) file: &'a str,
    pub(super) line: u32,
    pub(super) column: u32,
}
impl<'a> At<'a> {
    /// Parse `file:line:column`. Split from the right, because a path may
    /// itself contain a colon and the two numbers may not.
    fn parse(raw: &'a str) -> Option<Self> {
        let (head, column) = raw.rsplit_once(':')?;
        let (file, line) = head.rsplit_once(':')?;
        (!file.is_empty()).then_some(Self {
            file,
            line: line.parse().ok()?,
            column: column.parse().ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A served tree with a page, a clean-URL directory, a flat `.html`, and a
    /// secret sitting *outside* it.
    fn served() -> (tempfile::TempDir, Route) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dist = tmp.path().join("public");
        std::fs::create_dir_all(dist.join("posts/a")).expect("mkdir");
        std::fs::write(dist.join("index.html"), "home").expect("write");
        std::fs::write(dist.join("posts/a/index.html"), "a").expect("write");
        std::fs::write(dist.join("posts/flat.html"), "flat").expect("write");
        std::fs::write(tmp.path().join("secret.txt"), "not yours").expect("write");
        let route = Route {
            dist: crate::fs::canonicalize(&dist).expect("canonical"),
            base: String::new(),
            open: None,
            langs: Vec::new(),
        };
        (tmp, route)
    }

    #[test]
    fn a_url_resolves_to_the_file_under_dist() {
        let (_tmp, route) = served();
        for (url, expected) in [
            ("/", "index.html"),
            ("/index.html", "index.html"),
            ("/posts/a/", "posts/a/index.html"),
            ("/posts/a", "posts/a/index.html"),
            ("/posts/flat", "posts/flat.html"),
            ("/posts/a/?x=1", "posts/a/index.html"),
        ] {
            assert_eq!(route.resolve(url), Some(route.dist.join(expected)), "{url}");
        }
        assert_eq!(route.resolve("/nowhere/"), None);
    }

    /// A percent-encoded `..` is refused after decoding, which is the case
    /// decoding could have opened.
    #[test]
    fn a_request_cannot_escape_the_served_root() {
        let (tmp, route) = served();
        std::fs::write(tmp.path().join("secret.txt"), "not yours").expect("write");
        for url in [
            "/../secret.txt",
            "/posts/../../secret.txt",
            "/%2e%2e/secret.txt",
            "/%2E%2E%2Fsecret.txt",
            "/....//secret.txt",
            "/posts/a/../../../secret.txt",
        ] {
            assert_eq!(route.resolve(url), None, "{url} escaped the served root");
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_out_of_the_served_root_is_refused() {
        let (tmp, route) = served();
        std::os::unix::fs::symlink(tmp.path().join("secret.txt"), route.dist.join("leak.txt"))
            .expect("symlink");
        assert_eq!(route.resolve("/leak.txt"), None);

        std::os::unix::fs::symlink(route.dist.join("index.html"), route.dist.join("alias.html"))
            .expect("symlink");
        assert_eq!(
            route.resolve("/alias.html"),
            Some(route.dist.join("index.html"))
        );
    }

    #[test]
    fn a_directory_is_not_served_as_a_file() {
        let (_tmp, route) = served();
        assert_eq!(route.resolve("/posts"), None);
    }

    #[test]
    fn a_base_path_is_stripped_before_the_lookup() {
        let (_tmp, mut route) = served();
        route.base = "/docs".to_owned();
        assert_eq!(
            route.resolve("/docs/posts/a/"),
            Some(route.dist.join("posts/a/index.html"))
        );
    }

    #[test]
    fn an_empty_at_names_no_location_rather_than_a_bad_one() {
        assert_eq!(Handler::addressed("/__baudelaire/open"), None);
        assert_eq!(Handler::addressed("/__baudelaire/open?at="), None);
        assert_eq!(Handler::addressed("/__baudelaire/open?other=1"), None);
        assert_eq!(
            Handler::addressed("/__baudelaire/open?at=a.typ:1:2"),
            Some("a.typ:1:2")
        );
    }
}
