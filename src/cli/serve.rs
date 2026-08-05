//! Dev server: serve the built site, watch for changes, rebuild and live-reload.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;

use itertools::Itertools;
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};
use owo_colors::OwoColorize;
use tiny_http::{Header, Request, Response, Server};
use wax::{Glob, Program};

use crate::cli::Root;
use crate::config::{Config, Percent};
use crate::engine::{Engine, Mode};
use crate::error::serve::ServeError;
use crate::error::warning::{BrowserOpen, ConfigReload, RebuildFailed, WatchLost};
use crate::error::{ContentError, Result};
use crate::mime::Mime;
use crate::ui::{Level, Paths, Timer, Ui};

/// Run the dev server: build once, serve `dist`, and (unless `--no-watch`)
/// watch for changes to rebuild and live-reload browsers.
///
/// CLI flags (`--port`, `--bind`, `--open`, `--no-watch`) are already folded
/// into `config.serve` by [`crate::cli::ServeArgs::apply`].
pub(crate) fn run<'a>(
    ui: &'a Ui,
    config: Config,
    root: &'a Root,
    config_path: PathBuf,
    reload: impl FnMut() -> Result<Config> + 'a,
) -> Result<()> {
    Dev {
        config,
        ui,
        root,
        config_path,
        reload: Box::new(reload),
        tracked: Vec::new(),
        rewatch: false,
    }
    .run()
}

/// Orchestrates a dev-server session: the initial build, the HTTP handler, and
/// the watch/rebuild loop. Owns its [`Config`] so a change to `config.kdl` can
/// reload it (via [`Dev::reload`]) without restarting the process.
struct Dev<'a> {
    config: Config,
    ui: &'a Ui,
    root: &'a Root,
    /// The config file the session was started with (`--config`), watched so
    /// edits to it reload the session live.
    config_path: PathBuf,
    /// Re-reads `config.kdl` with the same profile and CLI overrides, invoked
    /// when the config file changes so edits take effect live.
    reload: Box<dyn FnMut() -> Result<Config> + 'a>,
    /// Directories the last build read outside the four source trees: a `data/`
    /// tree a page loaded, wherever the site keeps it. Watched on top of the
    /// defaults, so an input the build demonstrably depends on does not also
    /// have to be named in `serve { include }`.
    tracked: Vec<PathBuf>,
    /// Whether that set changed, so the watch loop re-registers with it.
    rewatch: bool,
}

impl Dev<'_> {
    fn run(mut self) -> Result<()> {
        let requested = format!("{}:{}", self.config.serve.bind, self.config.serve.port);
        let server = Server::http(&requested).map_err(|e| ServeError::bind(&requested, e))?;
        // What was bound, not what was asked for: `port 0` means "any free
        // port", and the banner used to answer that request by printing
        // `http://127.0.0.1:0/`.
        let addr = server
            .server_addr()
            .to_ip()
            .map_or(requested, |bound| bound.to_string());

        // A failed first build is a warning, not a fatal error, exactly like
        // every rebuild after it: the same typo killed the server or merely
        // warned depending only on when it was made. The server comes up and
        // fixing the file rebuilds.
        match self.rebuild() {
            Ok(stats) => self.tracked = stats.read,
            Err(e) => {
                self.ui.warn(RebuildFailed { errors: vec![e] });
                self.ui.flush();
            }
        }

        // Registered before anything is announced, and before a browser is
        // launched at it: the banner below promises the sources are watched,
        // and an edit saved between that promise and the registration would
        // have reached nobody. After the build, because the set of directories
        // to watch includes the ones the build turned out to read.

        // Registered before anything is announced, and before a browser is
        // launched at it: the banner below promises the sources are watched,
        // and an edit saved between that promise and the registration would
        // have reached nobody. After the build, because the set of directories
        // to watch includes the ones the build turned out to read.
        let watching = match self.config.serve.watch {
            true => Some(self.establish()?),
            false => None,
        };

        self.ui.blank();
        self.ui.arrow(
            "local",
            format!("http://{addr}{}/", self.config.base_path())
                .cyan()
                .underline(),
        );
        self.ui.arrow(
            "watching",
            match self.config.serve.watch {
                // Wrap the watched roots to the terminal, aligned under the
                // arrow's value column, so a long list flows onto extra lines
                // instead of running off-screen.
                true => crate::ui::wrap(
                    &self.watched(),
                    crate::ui::ARROW_VALUE_COLUMN,
                    crate::ui::term_width(),
                )
                .dimmed()
                .to_string(),
                false => "off (--no-watch)".dimmed().to_string(),
            },
        );
        self.ui.blank();
        if self.config.serve.open {
            // Detached: `open::that` waits for the spawned program to exit, so a
            // browser launched in the foreground would block the watch loop until
            // its window closed. Failing to open a browser is non-fatal (the
            // server is already up), so report it and carry on.
            let url = format!("http://{addr}{}/", self.config.base_path());
            if let Err(e) = open::that_detached(&url) {
                self.ui.warn(BrowserOpen { url, source: e });
                self.ui.flush();
            }
        }

        let level = self.ui.level();
        let route = Arc::new(Mutex::new(Route::new(&self.config)));
        if let Some(watching) = watching {
            let live = Live::default();
            Handler::new(Arc::clone(&route), Some(live.clone()), level).spawn(server);
            return self.watch(watching, &live, &route);
        }
        Handler::new(route, None, level).serve(&server);
        Ok(())
    }

    /// Build the site once.
    ///
    /// A fresh [`Engine`] every time, deliberately: its [`crate::world::Project`]
    /// memoizes file contents with no invalidation hook, so a reused one serves
    /// the bytes it first read and an edit never shows up. That costs six `git`
    /// subprocesses and the loaded fonts per rebuild; making it reusable means
    /// giving the file store a reset, not just hoisting the value.
    fn rebuild(&self) -> Result<crate::engine::Stats> {
        Engine::new(self.config.clone(), Mode::Serve)?.build(self.ui)
    }

    /// The watched roots, for the startup banner: the defaults, the config
    /// file, plus any `serve.include` globs. Returned as separate items so the
    /// banner can wrap them to the terminal width.
    fn watched(&self) -> Vec<String> {
        // The same roots the watcher registers, so the banner cannot advertise
        // a directory nothing watches.
        let mut parts: Vec<String> = Filter::roots(&self.config)
            .iter()
            .map(|dir| dir.display().to_string())
            .collect();
        parts.push(self.config_path.display().to_string());
        parts.extend(self.config.serve.include.iter().cloned());
        parts
    }

    /// Register the watcher and open the channel its events arrive on.
    ///
    /// Separate from [`Dev::watch`] so the caller can establish it *before*
    /// announcing the session. The banner says `watching content · templates ·
    /// ...`, and it used to say so while nothing was watching yet: the watcher
    /// came up after the banner, after the browser launch, and an edit saved in
    /// that window reached nobody. A file event is edge-triggered, so nothing
    /// later made up for it and the session simply ignored that save.
    ///
    /// Events arriving before the loop starts consuming are not lost: the
    /// channel is unbounded, and they are read as soon as it does.
    fn establish(&self) -> Result<Watching> {
        let filter =
            Filter::new(&self.config, self.root, &self.config_path)?.watching(&self.tracked);
        let (tx, rx) = flume::unbounded::<DebounceEventResult>();
        let watcher = Watcher::new(filter.watches(), tx)?;
        tracing::debug!(watches = ?filter.watches(), "watcher established");
        Ok(Watching {
            filter,
            rx,
            _watcher: watcher,
        })
    }

    /// Rebuild on every relevant change, until the watch channel closes.
    ///
    /// `watching` is the already-registered watcher. It is re-established
    /// whenever `config.kdl` is reloaded, so changes to watched roots
    /// (`serve.include`, paths) take effect. (A `bind`/`port` change still needs
    /// a restart: the HTTP server is already bound.)
    fn watch(mut self, mut watching: Watching, live: &Live, route: &Mutex<Route>) -> Result<()> {
        loop {
            self.rewatch = false;
            let mut reloaded = false;
            for result in &watching.rx {
                let outcome = self.on_event(result, live, &watching.filter);
                // Render whatever the iteration warned about (watcher trouble,
                // a failed rebuild) right away; the server runs indefinitely,
                // so there is no end-of-run flush to wait for.
                self.ui.flush();
                if outcome || self.rewatch {
                    reloaded = true;
                    break;
                }
            }
            if !reloaded {
                return Ok(());
            }
            // The reloaded config may have moved `dist` or changed `url`.
            *route.lock() = Route::new(&self.config);
            watching = self.establish()?;
        }
    }

    /// Handle one debounced watcher delivery: rebuild on events, and surface
    /// watcher failures (dropped watches, queue overflow) as warnings instead
    /// of silently discarding them; the server keeps serving either way.
    /// Returns whether `config.kdl` was reloaded (so the caller recreates the
    /// watcher).
    fn on_event(&mut self, result: DebounceEventResult, live: &Live, filter: &Filter) -> bool {
        match result {
            Ok(events) => self.on_change(&events, live, filter),
            Err(errors) => {
                for error in errors {
                    self.ui.warn(WatchLost { source: error });
                }
                false
            }
        }
    }

    /// Rebuild after a batch of file events, then push a live reload on success.
    fn on_change(&mut self, events: &[DebouncedEvent], live: &Live, filter: &Filter) -> bool {
        // A single edit can surface as several debounced events (and each event
        // may repeat the path), so dedupe before reporting or we print the file
        // once per raw event.
        let changed: Vec<_> = events
            .iter()
            .filter(|e| Self::is_content_change(e.event.kind))
            .flat_map(|e| e.event.paths.iter())
            .filter(|p| filter.is_relevant(p))
            .unique()
            .collect();
        if changed.is_empty() {
            return false;
        }

        // A change to the config file reloads it first, so the rebuild (and,
        // back in `watch`, the recreated watcher) see the new settings. A parse
        // error keeps the last-good config so the server stays up.
        let config_changed = changed.iter().any(|p| filter.is_config(p));
        if config_changed {
            match (self.reload)() {
                Ok(config) => self.config = config,
                Err(e) => {
                    // The parse error rides along as a related diagnostic, so
                    // the warning renders it in full, spans and all.
                    self.ui.warn(ConfigReload { errors: vec![e] });
                    return false;
                }
            }
        }

        // A vite-style rebuild: a transient status while the build runs, replaced
        // by a single timestamped log line. The build's own summary is silenced
        // so rebuilds never stack the full block over the initial output.
        let label = Self::label(&changed, self.root);
        tracing::debug!(?changed, "rebuilding");
        self.ui.status(format_args!("rebuilding {}", Paths(&label)));
        let timer = Timer::start();
        let prior = self.ui.level();
        self.ui.set_level(Level::Silent);
        let result = self.rebuild();
        self.ui.set_level(prior);

        match result {
            Ok(stats) => {
                // report what the rebuild recompiled, not the whole site.
                self.ui
                    .event(label, stats.pages - stats.cached, timer.elapsed());
                live.bump();
                // A build that read a new directory (a data file a page just
                // started loading) is watched from the next loop around.
                if stats.read != self.tracked {
                    self.tracked = stats.read;
                    self.rewatch = true;
                }
            }
            Err(e) => {
                // The failure rides along as a related diagnostic (spans,
                // offending page and all), rendered by the caller's flush...
                let failure = RebuildFailed { errors: vec![e] };
                // ...and the same text goes to every open tab, because the
                // browser is where the author is looking and it otherwise just
                // keeps showing the last good page, saying nothing.
                live.failed(&Ui::plain(&failure));
                self.ui.warn(failure);
            }
        }
        config_changed
    }

    /// A concise label for a rebuild's trigger: the first changed file (relative
    /// to the project root) and, when several changed, how many more.
    fn label(changed: &[&PathBuf], root: &Root) -> String {
        let first = changed[0]
            .strip_prefix(root.path())
            .unwrap_or(changed[0])
            .display();
        match changed.len() {
            1 => first.to_string(),
            n => format!("{first} +{}", n - 1),
        }
    }

    /// Whether an event actually changes content. Crucially excludes `Access`
    /// (and metadata) events: a rebuild *reads* every source, and reacting to
    /// those reads would loop the watcher forever.
    fn is_content_change(kind: notify::EventKind) -> bool {
        use notify::EventKind;
        use notify::event::ModifyKind;
        matches!(
            kind,
            EventKind::Create(_)
                | EventKind::Remove(_)
                | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Any)
        )
    }
}

/// Serves files from `dist`, optionally injecting live reload. Moved into the
/// request-handling thread, so it is `Send` and self-contained.
struct Handler {
    /// Shared with the rebuild loop, so a `config.kdl` reload that moves `dist`
    /// or changes `url` reaches the request thread; the handler outlives any
    /// single config, and used to serve the startup one forever.
    route: Arc<Mutex<Route>>,
    live: Option<Live>,
    /// The handler's own [`Ui`] at the session's verbosity, so per-request
    /// logging (404s) honors `--quiet` like every other line without sharing
    /// the rebuild loop's writer.
    ui: Ui,
}

/// What the server reads from, what URL prefix it strips, and how it opens a
/// source file: the config-derived values a reload can move out from under a
/// running handler.
#[derive(Clone)]
struct Route {
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
    /// with the French not-found page the build wrote there rather than the
    /// default language's. Empty on a single-language site.
    langs: Vec<String>,
}

impl Route {
    /// Resolve a URL path to a file under [`dist`](Route::dist), honoring clean
    /// URLs. Every candidate is checked to stay within it (see
    /// [`within`](Route::within)), so a `..`-laden or symlinked request can
    /// never escape the served root.
    ///
    /// On the route rather than on the handler because it is a question about
    /// the served tree and nothing else: no request, no server, no lock. It used
    /// to clone the whole route and then re-lock it once per candidate, which
    /// meant the path it resolved and the root it was checked against could come
    /// from two different configs across a reload, and it left the traversal
    /// check reachable only by making an HTTP request.
    fn resolve(&self, url: &str) -> Option<PathBuf> {
        let path = url.split('?').next().unwrap_or(url);
        let rel = path
            .strip_prefix(&self.base)
            .unwrap_or(path)
            .trim_start_matches('/');
        // Browsers percent-encode every non-ASCII byte, so a page whose slug
        // carries one (`/posts/café/`) arrives as `%C3%A9` and matches no file
        // on disk. `within` still canonicalizes and containment-checks whatever
        // this produces, so decoding cannot open a traversal.
        let rel = Percent::decode(rel);
        let base = self.dist.join(&rel);
        self.within(&base)
            .or_else(|| self.within(&base.join("index.html")))
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

    fn new(config: &Config) -> Self {
        let dist = config.paths.dist.clone();
        Self {
            dist: crate::fs::canonicalize(&dist).unwrap_or(dist),
            base: config.base_path().to_owned(),
            open: Open::new(config),
            langs: match config.multilingual() {
                true => config.langs().iter().map(|c| (*c).to_owned()).collect(),
                false => Vec::new(),
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
    fn new(route: Arc<Mutex<Route>>, live: Option<Live>, level: Level) -> Self {
        Self {
            route,
            live,
            ui: Ui::new(level),
        }
    }

    /// Run the request loop on its own thread (used while watching, so the main
    /// thread is free for the rebuild loop).
    fn spawn(self, server: Server) {
        std::thread::spawn(move || self.serve(&server));
    }

    /// Blocking request loop.
    fn serve(&self, server: &Server) {
        while let Ok(req) = server.recv() {
            self.handle(req);
        }
    }

    /// Serve the live-reload stream, open a source location, or map the URL to
    /// a file under `dist`.
    fn handle(&self, req: Request) {
        let url = req.url().to_owned();
        if let Some(live) = &self.live
            && url.starts_with(Live::ENDPOINT)
        {
            live.serve(req);
            return;
        }
        // Only while watching: the alt-click handler rides in with the
        // live-reload client, so a `--no-watch` session serves files and
        // nothing else.
        if self.live.is_some() && url.starts_with(Open::ENDPOINT) {
            self.open(req, &url);
            return;
        }
        // Bound before the match, not in it: the guard would otherwise live to
        // the end of the arm, and `respond_404` locks the route again.
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
        // A read failure (a permission error, or the file vanishing between the
        // `exists` check and here during a rebuild) is a 500, never a blank 200.
        let Ok(mut body) = crate::fs::read(path) else {
            // Logged like a 404: an unreadable file used to produce a blank page
            // and an idle-looking server, with no line at any verbosity.
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
        // A per-request line at the session's level, so `--quiet` silences
        // these too. (It may still interleave with a concurrent rebuild
        // status line; acceptable for now.)
        self.ui.request(404, url);
        // The build writes one not-found page per language; a request inside a
        // language's own subtree gets that language's, and everything else the
        // default one. One lock for the whole lookup, so the page and the root
        // it is checked against come from the same route.
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

    /// Hand a stamped source location to the configured editor.
    ///
    /// The location was written by this build, into a `data-typst` attribute,
    /// but it arrives as a request and is checked as one: the browser must be
    /// calling from the page itself, the location must parse, and the file it
    /// names must be one of the project's own.
    fn open(&self, req: Request, url: &str) {
        let outcome = self.launch(&req, url);
        let status = outcome.as_ref().err().map_or(200, Unopenable::status);
        self.ui.request(status, url);
        // The refusal is the response body: the author is looking at the
        // browser (they just clicked in it), which is where the injected
        // client puts this, exactly as it does a failed rebuild.
        let body = outcome.err().map(|why| why.body()).unwrap_or_default();
        let _ = req.respond(Response::from_string(body).with_status_code(status));
    }

    fn launch(&self, req: &Request, url: &str) -> Result<(), Unopenable> {
        if !Self::same_origin(req) {
            return Err(Unopenable::Foreign);
        }
        let raw = Self::query(url, "at").ok_or_else(|| Unopenable::Malformed(url.to_owned()))?;
        let decoded = Percent::decode(raw);
        let at = At::parse(&decoded).ok_or_else(|| Unopenable::Malformed(decoded.clone()))?;
        let open = self.route.lock().open.clone();
        open.ok_or(Unopenable::Unconfigured)?.at(&at)
    }

    /// Whether the request came from the page this server served.
    ///
    /// A browser labels every request: the injected client's `fetch` is
    /// `same-origin`, and anything another site provokes is not. A client that
    /// sends no label at all (curl, a test) is allowed through: it is already
    /// running as the author, which is all this endpoint's authority amounts to.
    fn same_origin(req: &Request) -> bool {
        req.headers()
            .iter()
            .find(|header| header.field.equiv("Sec-Fetch-Site"))
            .is_none_or(|header| header.value.as_str() == "same-origin")
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

/// The source-opening endpoint's path, a macro for the same reason
/// [`live_endpoint`] is one: the literal is both matched per request and
/// `concat!`ed into the client that calls it.
macro_rules! open_endpoint {
    () => {
        "/__baudelaire/open"
    };
}

/// A source location as a stamped element spells it: `file:line:column`, the
/// value [`crate::render`] writes into `data-typst`.
struct At<'a> {
    /// Project-relative, as the compiler named the file.
    file: &'a str,
    line: u32,
    column: u32,
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

/// Opens a source location in the author's editor, for a preview alt-click.
///
/// Exists only when `serve { editor .. }` names a command: nothing is guessed
/// from the environment, and with no command configured the endpoint answers
/// with what to configure instead of launching something the author did not ask
/// for.
#[derive(Clone)]
struct Open {
    /// The project root, canonical. Every requested file is resolved against it
    /// and must stay inside it: the request names a path, and this is what
    /// keeps it to the site's own sources.
    root: PathBuf,
    /// The program, then each of its arguments. Run directly, never through a
    /// shell, so a path with a space or a semicolon in it is an argument and
    /// can never be a second command.
    command: Vec<String>,
}

impl Open {
    /// Endpoint the injected client posts a location to.
    const ENDPOINT: &'static str = open_endpoint!();

    fn new(config: &Config) -> Option<Self> {
        let root = config.root.clone();
        (!config.serve.editor.is_empty()).then(|| Self {
            root: crate::fs::canonicalize(&root).unwrap_or(root),
            command: config.serve.editor.clone(),
        })
    }

    /// Run the editor at `at`, once the file it names is confirmed to be one of
    /// the project's own.
    fn at(&self, at: &At<'_>) -> Result<(), Unopenable> {
        let path = crate::fs::canonicalize(self.root.join(at.file))
            .ok()
            .filter(|path| path.starts_with(&self.root) && path.is_file())
            .ok_or_else(|| Unopenable::Outside(at.file.to_owned()))?;
        let mut words = self.substituted(&path.display().to_string(), at);
        let program = words.remove(0);
        let mut child = Command::new(&program)
            .args(words)
            .current_dir(&self.root)
            // The dev server owns this terminal; an editor writing into it
            // would land in the middle of the rebuild log.
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| Unopenable::Spawn {
                program: program.clone(),
                source,
            })?;
        // Reaped on its own thread: waiting here would hold the request open
        // for as long as the editor runs, and not waiting at all leaves a
        // zombie behind every click.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }

    /// The command with `{file}`, `{line}` and `{column}` filled in. Per word,
    /// so a value lands inside whatever argument shape an editor wants
    /// (`+{line}`, `--goto {file}:{line}:{column}`) and never splits into two.
    fn substituted(&self, file: &str, at: &At<'_>) -> Vec<String> {
        self.command
            .iter()
            .map(|word| {
                word.replace("{file}", file)
                    .replace("{line}", &at.line.to_string())
                    .replace("{column}", &at.column.to_string())
            })
            .collect()
    }
}

/// Why a source location could not be opened. Its text is the response body, so
/// it addresses the author at the browser: what to fix, not what failed inside.
#[derive(thiserror::Error, Debug)]
enum Unopenable {
    #[error("refused: this endpoint only answers the page it was served with")]
    Foreign,
    #[error("no editor configured")]
    Unconfigured,
    #[error("not a source location: {0}")]
    Malformed(String),
    #[error("{0} is not a file in this project")]
    Outside(String),
    #[error("could not run {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
}

impl Unopenable {
    /// The status the refusal answers with: a browser distinguishes them, and
    /// the client shows the body either way.
    fn status(&self) -> u16 {
        match self {
            Self::Foreign => 403,
            Self::Unconfigured => 501,
            Self::Malformed(_) => 400,
            Self::Outside(_) => 404,
            Self::Spawn { .. } => 500,
        }
    }

    /// What to do about it, when there is something to do. A spawn failure is
    /// the one worth guessing at: an editor the server cannot find is almost
    /// always one that is not installed, or not on the `PATH` this process
    /// inherited (a desktop-launched terminal often has a shorter one).
    fn help(&self) -> Option<&'static str> {
        match self {
            Self::Foreign => None,
            Self::Unconfigured => Some(
                "add `serve { editor \"code\" \"--goto\" \"{file}:{line}:{column}\" }` to config.kdl",
            ),
            Self::Malformed(_) => Some("a stamped element reads `file:line:column`"),
            Self::Outside(_) => Some("only files under the project root can be opened"),
            Self::Spawn { source, .. } => match source.kind() {
                std::io::ErrorKind::NotFound => {
                    Some("is it installed, and on the PATH this dev server inherited?")
                }
                _ => None,
            },
        }
    }

    /// The response body, in the shape a diagnostic has: a marked headline and
    /// a `help:` line. The injected client lays a refusal out with the same
    /// renderer it lays a failed rebuild out with, so one shape here means one
    /// presentation there rather than a special case per message.
    fn body(&self) -> String {
        use std::fmt::Write as _;

        let mut text = format!("× {self}");
        if let Some(help) = self.help() {
            let _ = write!(text, "\nhelp: {help}");
        }
        text
    }
}

/// Live-reload coordination between the request handler and the rebuild loop.
///
/// The handler injects [`Live::SCRIPT`] into HTML responses; the injected
/// client opens a Server-Sent Events stream at [`Live::ENDPOINT`]. Each
/// successful rebuild calls [`Live::bump`], pushing a reload to every open
/// stream.
///
/// Streams are keyed by id so a closed connection is reaped promptly: the writer
/// thread wakes every [`Live::HEARTBEAT`] to send an SSE comment, notices the
/// dead socket on the failed write, and removes its own entry: no leak waiting
/// on the next rebuild.
#[derive(Clone, Default)]
struct Live {
    /// One sender per open SSE connection, keyed for self-removal on close.
    streams: Arc<Mutex<HashMap<u64, flume::Sender<Signal>>>>,
    /// Monotonic source of stream ids.
    next_id: Arc<AtomicU64>,
}

/// The live-reload endpoint's path, as a macro so the one literal serves both
/// [`Live::ENDPOINT`] (matched per request) and the client script that connects
/// to it: the script is a `const`, and only a literal can be `concat!`ed into
/// one. The two used to be separate literals that had to be kept equal by hand.
macro_rules! live_endpoint {
    () => {
        "/__baudelaire/live"
    };
}

impl Live {
    /// Endpoint the injected client connects to for the reload event stream.
    const ENDPOINT: &'static str = live_endpoint!();

    /// How often an idle stream emits a keep-alive comment. Doubles as the upper
    /// bound on how long a closed connection lingers before it is reaped.
    const HEARTBEAT: Duration = Duration::from_secs(10);

    /// Client script appended to served HTML.
    ///
    /// One file per piece, composed here: the DOM helpers, the diagnostic
    /// renderer, the panel a message is shown in, the reload stream with its
    /// status dot, and the alt-click that opens a stamped element's source.
    /// Each is a lambda, so the block
    /// scope below is all they share and the endpoint literals reach them
    /// through the same `concat!` that keeps [`Live::ENDPOINT`] and
    /// [`Open::ENDPOINT`] in agreement with the client that calls them.
    const SCRIPT: &'static str = concat!(
        "\n<script>\n{\n",
        "const dom = (",
        include_str!("js/dom.js"),
        ")();\n",
        "const report = (",
        include_str!("js/report.js"),
        ")(dom);\n",
        "const overlay = (",
        include_str!("js/overlay.js"),
        ")(dom, report);\n",
        "(",
        include_str!("js/live.js"),
        ")('",
        live_endpoint!(),
        "', overlay, dom);\n",
        "(",
        include_str!("js/source.js"),
        ")('",
        open_endpoint!(),
        "', overlay);\n",
        "}\n</script>\n"
    );

    /// Raw HTTP response head that opens an SSE stream, plus a comment so the
    /// client registers the connection immediately.
    const HEAD: &'static str = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream\r\n\
        Cache-Control: no-cache\r\n\
        Connection: keep-alive\r\n\
        \r\n\
        : ok\n\n";

    /// Advance every open stream, dropping any whose client has gone.
    fn bump(&self) {
        self.push(&Signal::Reload);
    }

    /// Put a failed rebuild's diagnostic on screen in every open tab.
    ///
    /// The terminal already says this; the browser did not, and the browser is
    /// where the author is looking. `text` is the same rendered diagnostic,
    /// plain, carried as a JSON string so it survives SSE's line framing.
    fn failed(&self, text: &str) {
        let payload = serde_json::to_string(text).unwrap_or_else(|_| String::from("\"\""));
        self.push(&Signal::Failed(payload));
    }

    fn push(&self, signal: &Signal) {
        self.streams
            .lock()
            .retain(|_, tx| tx.send(signal.clone()).is_ok());
    }

    /// Open an SSE stream for `req` on its own thread, writing directly to the
    /// socket so each event flushes the instant a rebuild finishes. The thread
    /// removes its own entry when it ends, so a closed tab frees its slot within
    /// one [`Live::HEARTBEAT`] instead of lingering until the next rebuild.
    fn serve(&self, req: Request) {
        let (tx, signals) = flume::unbounded();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.streams.lock().insert(id, tx);
        let streams = Arc::clone(&self.streams);
        std::thread::spawn(move || {
            let mut socket = req.into_writer();
            if socket.write_all(Self::HEAD.as_bytes()).is_ok() && socket.flush().is_ok() {
                loop {
                    // A rebuild pushes `reload`; an idle timeout emits a comment
                    // keep-alive whose failed write reveals a closed socket.
                    let payload = match signals.recv_timeout(Self::HEARTBEAT) {
                        Ok(signal) => signal.frame(),
                        Err(flume::RecvTimeoutError::Timeout) => ": ping\n\n".to_owned(),
                        Err(flume::RecvTimeoutError::Disconnected) => break,
                    };
                    if socket.write_all(payload.as_bytes()).is_err() || socket.flush().is_err() {
                        break;
                    }
                }
            }
            streams.lock().remove(&id);
        });
    }
}

/// What a rebuild pushes down an open live-reload stream.
#[derive(Debug, Clone)]
enum Signal {
    /// The rebuild succeeded: reload the page.
    Reload,
    /// It did not, carrying the rendered diagnostic as a JSON string.
    Failed(String),
}

impl Signal {
    /// This signal as an SSE frame. The default (unnamed) event stays `reload`,
    /// so a client from before the overlay existed still reloads.
    fn frame(&self) -> String {
        match self {
            Self::Reload => "data: reload\n\n".to_owned(),
            Self::Failed(json) => format!("event: failed\ndata: {json}\n\n"),
        }
    }
}

/// Debounced file watcher over the session's watch roots.
struct Watcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

/// A live registration: the watcher, what it decided to watch, and the channel
/// its debounced events arrive on. The three travel together because they are
/// only meaningful together, and because dropping the [`Watcher`] is what
/// unregisters it.
struct Watching {
    filter: Filter,
    rx: flume::Receiver<DebounceEventResult>,
    _watcher: Watcher,
}

impl Watcher {
    fn new(
        watches: &[(PathBuf, notify::RecursiveMode)],
        tx: flume::Sender<DebounceEventResult>,
    ) -> Result<Self> {
        let handler = move |result: DebounceEventResult| {
            let _ = tx.send(result);
        };
        let mut debouncer = new_debouncer(Duration::from_millis(500), None, handler)
            .map_err(ServeError::watcher_init)?;
        for (dir, mode) in watches {
            if dir.exists() {
                debouncer
                    .watch(dir, *mode)
                    .map_err(|e| ServeError::watch(dir, e))?;
            }
        }
        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

/// Decides which changed paths trigger a rebuild, and which roots to watch.
/// Built once per session from `serve.include` / `serve.exclude` wax globs:
/// `exclude` (e.g. hook-generated files) wins over everything, then `include`
/// adds sources beyond the defaults (content, templates, assets, and the
/// config file itself).
struct Filter {
    root: PathBuf,
    assets: PathBuf,
    statics: PathBuf,
    /// The session's config file, absolute (canonical when it resolves), so a
    /// changed path can be tested for "is this *my* config": a sibling `.kdl`
    /// in the same directory must not reload the session.
    config: PathBuf,
    watches: Vec<(PathBuf, notify::RecursiveMode)>,
    include: Vec<Glob<'static>>,
    exclude: Vec<Glob<'static>>,
    /// Directories the last build read outside the watched trees (see
    /// [`Filter::watching`]), so an event in one is relevant without a glob.
    tracked: Vec<PathBuf>,
    /// The build's whole scratch tree, which is never an input however much it
    /// looks like one. See [`Filter::is_relevant`].
    scratch: PathBuf,
}

impl Filter {
    fn new(config: &Config, root: &Root, config_path: &Path) -> Result<Self> {
        use notify::RecursiveMode::{NonRecursive, Recursive};
        let root = root.path().to_path_buf();
        let assets = Self::absolute(&root, &config.paths.assets);
        // `Config::SCRATCH`, not `cache.dir`: the cache is one subdirectory of
        // the scratch tree, and the generated typst that caused the loop is a
        // sibling of it.
        let cache = Self::absolute(&root, Path::new(crate::config::Config::SCRATCH));
        let statics = Self::absolute(&root, &config.paths.r#static);
        let mut watches: Vec<_> = Self::roots(config)
            .iter()
            .map(|dir| (Self::absolute(&root, dir), Recursive))
            .collect();
        // Watch the config file via its parent directory, non-recursively:
        // editors commonly save by rename-over, which drops a watch pinned to
        // the file itself. A bare `config.kdl` has an empty parent, meaning
        // the project root.
        let config_dir = match config_path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => Self::absolute(&root, dir),
            _ => root.clone(),
        };
        watches.push((config_dir, NonRecursive));
        let config_file = Self::absolute(&root, config_path);
        let include = Self::compile(&config.serve.include)?;
        // Watch the literal prefix directory of each include glob (e.g. `data/`
        // in `data/**/*.json`) so its files are actually observed.
        for glob in &include {
            let (prefix, _) = glob.clone().partition();
            if !prefix.as_os_str().is_empty() {
                watches.push((Self::absolute(&root, &prefix), Recursive));
            }
        }
        let exclude = Self::compile(&config.serve.exclude)?;
        Ok(Self {
            root,
            assets,
            statics,
            config: config_file,
            watches,
            include,
            exclude,
            tracked: Vec::new(),
            scratch: cache,
        })
    }

    /// Also watch `dirs`, the directories the last build read outside those
    /// trees, and treat what they hold as relevant.
    ///
    /// Additive on purpose: `serve { include }` still covers what no compile
    /// reads (a `tsconfig.json`, a hook's input) and what a *failed* first build
    /// never got far enough to record. This only removes the need to repeat what
    /// the build already proved it depends on.
    fn watching(mut self, dirs: &[PathBuf]) -> Self {
        for dir in dirs {
            self.watches
                .push((dir.clone(), notify::RecursiveMode::Recursive));
        }
        self.tracked = dirs.to_vec();
        self
    }

    /// The source trees a session always watches, in the configured (relative)
    /// spelling. THE single list of them: both the watcher and the startup
    /// banner read it, so the two can never disagree.
    fn roots(config: &Config) -> [&Path; 4] {
        let paths = &config.paths;
        [
            &paths.content,
            &paths.templates,
            &paths.assets,
            &paths.r#static,
        ]
    }

    /// A path in the one form this filter compares in: absolute, canonical when
    /// it resolves, else root-joined.
    ///
    /// Watch roots go through here too, not just the comparison bases: a
    /// watcher reports events under the path it was registered with, so a
    /// symlinked `assets` (registered as the link, compared against its target)
    /// makes every branch of [`Self::is_relevant`] miss and no edit there
    /// rebuilds. Registering the resolved path keeps the two spellings equal by
    /// construction rather than by each backend's normalization.
    fn absolute(root: &Path, path: &Path) -> PathBuf {
        // Resolve against the project root, not the process cwd: a configured
        // path is root-relative by definition. Canonicalizing the bare path
        // happened to work only because the CLI has already chdir'd to the
        // root, and silently resolved to a same-named directory next to
        // wherever the process started when it had not.
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        crate::fs::canonicalize(&joined).unwrap_or(joined)
    }

    /// Compile a list of patterns into owned globs.
    fn compile(patterns: &[String]) -> Result<Vec<Glob<'static>>> {
        patterns
            .iter()
            .map(|pattern| {
                Glob::new(pattern)
                    .map(Glob::into_owned)
                    .map_err(|e| ContentError::bad_glob("serve", pattern, e).into())
            })
            .collect()
    }

    fn watches(&self) -> &[(PathBuf, notify::RecursiveMode)] {
        &self.watches
    }

    /// Whether a changed path is the session's config file. Canonicalized when
    /// possible so a symlinked event path still matches; falls back to a raw
    /// compare when the file is mid-rename (deleted, about to reappear).
    fn is_config(&self, path: &Path) -> bool {
        crate::fs::canonicalize(path).map_or(path == self.config, |p| p == self.config)
    }

    /// Whether a changed path should trigger a rebuild. Of `.kdl` files only
    /// the session's own config counts: baudelaire reads no other KDL, and
    /// the config directory's non-recursive watch also surfaces its siblings.
    fn is_relevant(&self, path: &Path) -> bool {
        // Nothing the build writes for itself can trigger it. `.baudelaire/`
        // holds generated typst the templates genuinely *import*, so the build
        // records it as a file it read and the watcher dutifully watched it --
        // and every build rewrites it, so every build queued the next one and
        // the session span until it was killed.
        //
        // Nothing is lost by ignoring it: those files are derived from content
        // and templates, which are watched, so the edit that changes one is
        // already a rebuild on its own account.
        if path.starts_with(&self.scratch) {
            return false;
        }
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        if self.exclude.iter().any(|g| g.is_match(rel)) {
            return false;
        }
        if self.include.iter().any(|g| g.is_match(rel)) {
            return true;
        }
        self.is_config(path)
            || path.extension().is_some_and(|e| e == "typ")
            || path.starts_with(&self.assets)
            || path.starts_with(&self.statics)
            || self.tracked.iter().any(|dir| path.starts_with(dir))
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

    /// The three shapes a URL can name a file by.
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

    /// The traversal check, asked directly rather than through a spawned server
    /// and `curl`. A `..` is refused however it is spelled, and a percent-encoded
    /// one is refused *after* decoding, which is the case decoding could have
    /// opened.
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

    /// A symlink is followed to where it actually points, so one aimed out of
    /// the tree is refused even though every path segment looks innocent. The
    /// reason the check canonicalizes rather than comparing strings.
    #[test]
    #[cfg(unix)]
    fn a_symlink_out_of_the_served_root_is_refused() {
        let (tmp, route) = served();
        std::os::unix::fs::symlink(tmp.path().join("secret.txt"), route.dist.join("leak.txt"))
            .expect("symlink");
        assert_eq!(route.resolve("/leak.txt"), None);

        // ...and one pointing back inside is served, so the check is
        // containment and not a blanket refusal of symlinks.
        std::os::unix::fs::symlink(route.dist.join("index.html"), route.dist.join("alias.html"))
            .expect("symlink");
        assert_eq!(
            route.resolve("/alias.html"),
            Some(route.dist.join("index.html"))
        );
    }

    /// A directory is not a file: the check answers for what would be read, so
    /// a request naming one falls through rather than serving a listing.
    #[test]
    fn a_directory_is_not_served_as_a_file() {
        let (_tmp, route) = served();
        assert_eq!(route.resolve("/posts"), None);
    }

    /// The base path is stripped before resolution, so a subdirectory-hosted
    /// site previews at the URLs it will really be served at.
    #[test]
    fn a_base_path_is_stripped_before_the_lookup() {
        let (_tmp, mut route) = served();
        route.base = "/docs".to_owned();
        assert_eq!(
            route.resolve("/docs/posts/a/"),
            Some(route.dist.join("posts/a/index.html"))
        );
    }

    /// Watcher failures are reported as warnings and do not stop the watch
    /// loop (previously the `Err` arm was silently discarded).
    #[test]
    fn watcher_errors_warn_and_keep_watching() {
        let config = Config::default();
        let root = Root::at(".");
        let ui = Ui::new(Level::Silent);
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();
        let live = Live::default();
        let mut dev = Dev {
            config,
            ui: &ui,
            root: &root,
            config_path: PathBuf::from("config.kdl"),
            reload: Box::new(|| Ok(Config::default())),
            tracked: Vec::new(),
            rewatch: false,
        };
        dev.on_event(Err(vec![notify::Error::generic("boom")]), &live, &filter);
        assert_eq!(ui.warnings(), 1);
    }

    /// The `Ok` arm still flows into change handling: irrelevant (empty) event
    /// batches are a no-op and produce no warnings.
    #[test]
    fn empty_event_batch_is_a_no_op() {
        let config = Config::default();
        let root = Root::at(".");
        let ui = Ui::new(Level::Silent);
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();
        let live = Live::default();
        let mut dev = Dev {
            config,
            ui: &ui,
            root: &root,
            config_path: PathBuf::from("config.kdl"),
            reload: Box::new(|| Ok(Config::default())),
            tracked: Vec::new(),
            rewatch: false,
        };
        dev.on_event(Ok(Vec::new()), &live, &filter);
        assert_eq!(ui.warnings(), 0);
    }

    /// The config file's directory is watched (non-recursively), so an edit to
    /// `config.kdl` at the project root reaches the reload path; it lives
    /// outside content/templates/assets, which are the only recursive roots.
    #[test]
    fn config_directory_is_watched_and_config_edits_are_relevant() {
        let config = Config::default();
        let root = Root::at("/proj");
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();
        assert!(
            filter
                .watches()
                .iter()
                .any(|(dir, mode)| dir == Path::new("/proj")
                    && *mode == notify::RecursiveMode::NonRecursive),
            "project root not watched for the config file: {:?}",
            filter.watches()
        );
        assert!(filter.is_relevant(Path::new("/proj/config.kdl")));
        // Unrelated root-level files seen via the same non-recursive watch do
        // not trigger rebuilds, not even other `.kdl` files: baudelaire reads
        // no KDL besides its config.
        assert!(!filter.is_relevant(Path::new("/proj/README.md")));
        assert!(!filter.is_relevant(Path::new("/proj/other.kdl")));
        assert!(filter.is_config(Path::new("/proj/config.kdl")));
        assert!(!filter.is_config(Path::new("/proj/other.kdl")));
    }

    /// Watch roots are registered resolved and absolute, in the same form
    /// `is_relevant` compares against: a watcher reports events under the path
    /// it was given, so the two must agree by construction.
    #[test]
    fn watch_roots_are_absolute_and_asset_edits_are_relevant() {
        let config = Config::default();
        let root = Root::at("/proj");
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();
        assert!(
            filter.watches().iter().all(|(dir, _)| dir.is_absolute()),
            "relative watch root: {:?}",
            filter.watches()
        );
        assert!(
            filter.is_relevant(
                &Path::new("/proj")
                    .join(&config.paths.assets)
                    .join("style.css")
            )
        );
        assert!(
            filter.is_relevant(
                &Path::new("/proj")
                    .join(&config.paths.r#static)
                    .join("CNAME")
            )
        );
        assert!(!filter.is_relevant(Path::new("/proj/elsewhere/style.css")));
    }

    /// Nothing the build writes for itself may trigger it.
    ///
    /// The scratch tree holds generated typst that templates genuinely
    /// `#import`, so the build records it as a file it *read* and the watcher
    /// dutifully watched it -- and every build rewrites it, so every build
    /// queued the next one. A `serve` session span at ~140ms a lap, rebuilding
    /// nothing, until it was killed.
    ///
    /// The whole tree, not just `cache.dir`: the cache is one subdirectory of
    /// it, and the file that caused the loop is a sibling of the cache.
    #[test]
    fn the_builds_own_scratch_tree_never_triggers_a_rebuild() {
        let config = Config::default();
        let root = Root::at("/proj");
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();
        let scratch = Path::new("/proj").join(Config::SCRATCH);

        assert!(
            !filter.is_relevant(&scratch.join("generated/sections.typ")),
            "a generated import rebuilt the site that generates it"
        );
        assert!(!filter.is_relevant(&scratch.join("generated/baudelaire.d.ts")));
        assert!(!filter.is_relevant(&config.cache.dir.join("manifest.json")));
        // ...and the content it is derived from still does, which is what makes
        // ignoring it lossless.
        assert!(
            filter.is_relevant(&Path::new("/proj").join(&config.paths.content).join("a.typ")),
            "the edit that regenerates it has to rebuild on its own account"
        );
    }

    /// A `--config` outside the root watches that file's own directory, and
    /// only that exact file: a sibling `.kdl` there must not reload the
    /// session or trigger a rebuild.
    #[test]
    fn relocated_config_watches_its_parent() {
        let config = Config::default();
        let root = Root::at("/proj");
        let filter = Filter::new(&config, &root, Path::new("/etc/baudelaire/prod.kdl")).unwrap();
        assert!(
            filter
                .watches()
                .iter()
                .any(|(dir, mode)| dir == Path::new("/etc/baudelaire")
                    && *mode == notify::RecursiveMode::NonRecursive),
            "config parent not watched: {:?}",
            filter.watches()
        );
        assert!(filter.is_config(Path::new("/etc/baudelaire/prod.kdl")));
        assert!(filter.is_relevant(Path::new("/etc/baudelaire/prod.kdl")));
        assert!(!filter.is_relevant(Path::new("/etc/baudelaire/other-site.kdl")));
    }

    /// A stream whose client is gone is reaped on the next bump, rather than
    /// accumulating in the registry.
    #[test]
    fn bump_reaps_streams_whose_client_disconnected() {
        let live = Live::default();
        let (live_tx, live_rx) = flume::unbounded();
        let (dead_tx, dead_rx) = flume::unbounded::<Signal>();
        live.streams.lock().insert(0, live_tx);
        live.streams.lock().insert(1, dead_tx);
        // The dead stream's receiver (its writer thread) is gone.
        drop(dead_rx);

        live.bump();

        let streams = live.streams.lock();
        assert!(streams.contains_key(&0), "live stream kept");
        assert!(!streams.contains_key(&1), "disconnected stream reaped");
        // The surviving stream received the reload signal.
        assert!(matches!(live_rx.try_recv(), Ok(Signal::Reload)));
    }

    /// A failed rebuild reaches the browser too. It used to reach only the
    /// terminal, so a tab kept showing the last good page with no hint that the
    /// save had not taken.
    #[test]
    fn a_failed_rebuild_pushes_its_diagnostic_to_open_tabs() {
        let live = Live::default();
        let (tx, rx) = flume::unbounded();
        live.streams.lock().insert(0, tx);

        live.failed("expected `}`\n  at line 3");

        let Ok(signal) = rx.try_recv() else {
            panic!("the open stream should have been signalled");
        };
        let frame = signal.frame();
        // A named event, so it is distinguishable from `EventSource`'s own
        // transport errors, and JSON-encoded so the newline survives SSE's
        // line framing intact.
        assert!(frame.starts_with("event: failed\ndata: "), "{frame}");
        assert!(frame.contains(r"expected `}`\n  at line 3"), "{frame}");
        assert!(frame.ends_with("\n\n"), "{frame}");
        // Exactly one `data:` line: an unencoded newline would split the frame
        // and the client would parse half a diagnostic.
        assert_eq!(frame.matches("data: ").count(), 1, "{frame}");
    }
}
