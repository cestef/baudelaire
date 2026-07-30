//! Dev server: serve the built site, watch for changes, rebuild and live-reload.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
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
        if let Err(e) = self.rebuild() {
            self.ui.warn(RebuildFailed { errors: vec![e] });
            self.ui.flush();
        }
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
        if self.config.serve.watch {
            let live = Live::default();
            Handler::new(Arc::clone(&route), Some(live.clone()), level).spawn(server);
            self.watch(live, &route)
        } else {
            Handler::new(route, None, level).serve(&server);
            Ok(())
        }
    }

    /// Build the site once.
    ///
    /// A fresh [`Engine`] every time, deliberately: its [`crate::world::Project`]
    /// memoizes file contents with no invalidation hook, so a reused one serves
    /// the bytes it first read and an edit never shows up. That costs six `git`
    /// subprocesses and the loaded fonts per rebuild; making it reusable means
    /// giving the file store a reset, not just hoisting the value.
    fn rebuild(&mut self) -> Result<crate::engine::Stats> {
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

    /// Watch content, templates, assets, and any `include` globs, rebuilding on
    /// every relevant change.
    fn watch(mut self, live: Live, route: &Mutex<Route>) -> Result<()> {
        // Rebuild the watcher whenever `config.kdl` is reloaded, so changes to
        // watched roots (`serve.include`, paths) take effect. (A `bind`/`port`
        // change still needs a restart: the HTTP server is already bound.)
        loop {
            let filter = Filter::new(&self.config, self.root, &self.config_path)?;
            let (tx, rx) = flume::unbounded::<DebounceEventResult>();
            let _watcher = Watcher::new(filter.watches(), tx)?;
            tracing::debug!(watches = ?filter.watches(), "watcher established");
            let mut reloaded = false;
            for result in rx {
                let outcome = self.on_event(result, &live, &filter)?;
                // Render whatever the iteration warned about (watcher trouble,
                // a failed rebuild) right away; the server runs indefinitely,
                // so there is no end-of-run flush to wait for.
                self.ui.flush();
                if outcome {
                    reloaded = true;
                    break;
                }
            }
            if !reloaded {
                return Ok(());
            }
            // The reloaded config may have moved `dist` or changed `url`.
            *route.lock() = Route::new(&self.config);
        }
    }

    /// Handle one debounced watcher delivery: rebuild on events, and surface
    /// watcher failures (dropped watches, queue overflow) as warnings instead
    /// of silently discarding them; the server keeps serving either way.
    /// Returns whether `config.kdl` was reloaded (so the caller recreates the
    /// watcher).
    fn on_event(
        &mut self,
        result: DebounceEventResult,
        live: &Live,
        filter: &Filter,
    ) -> Result<bool> {
        match result {
            Ok(events) => self.on_change(events, live, filter),
            Err(errors) => {
                for error in errors {
                    self.ui.warn(WatchLost { source: error });
                }
                Ok(false)
            }
        }
    }

    /// Rebuild after a batch of file events, then push a live reload on success.
    fn on_change(
        &mut self,
        events: Vec<DebouncedEvent>,
        live: &Live,
        filter: &Filter,
    ) -> Result<bool> {
        // A single edit can surface as several debounced events (and each event
        // may repeat the path), so dedupe before reporting or we print the file
        // once per raw event.
        let changed: Vec<_> = events
            .iter()
            .filter(|e| Self::is_content_change(&e.event.kind))
            .flat_map(|e| e.event.paths.iter())
            .filter(|p| filter.is_relevant(p))
            .unique()
            .collect();
        if changed.is_empty() {
            return Ok(false);
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
                    return Ok(false);
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
        Ok(config_changed)
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
    fn is_content_change(kind: &notify::EventKind) -> bool {
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

/// What the server reads from and what URL prefix it strips: the two things a
/// config reload can move out from under a running handler.
#[derive(Clone)]
struct Route {
    /// The served root, canonical so every per-request traversal check compares
    /// canonical paths (with `..` and symlinks resolved) against it.
    dist: PathBuf,
    /// The path the site is served under, stripped from each request so a
    /// subdirectory-hosted site (`url "https://host/docs"`) previews locally.
    base: String,
}

impl Route {
    fn new(config: &Config) -> Self {
        let dist = config.paths.dist.clone();
        Self {
            dist: crate::fs::canonicalize(&dist).unwrap_or(dist),
            base: config.base_path().to_owned(),
        }
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

    /// Serve the live-reload stream, or map the URL to a file under `dist`.
    fn handle(&self, req: Request) {
        let url = req.url().to_owned();
        if let Some(live) = &self.live
            && url.starts_with(Live::ENDPOINT)
        {
            live.serve(req);
            return;
        }
        match self.resolve(&url) {
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
        let dist = self.route.lock().dist.clone();
        match self.within(&dist.join(crate::config::Config::NOT_FOUND)) {
            Some(page) => self.serve_file(req, &page, 404),
            None => {
                let _ = req.respond(Response::empty(404));
            }
        }
    }

    /// Resolve a URL path to a file under `dist`, honoring clean URLs. Every
    /// candidate is checked to stay within `dist` (see [`Handler::within`]), so
    /// a `..`-laden or symlinked request can never escape the served root.
    fn resolve(&self, url: &str) -> Option<PathBuf> {
        let route = self.route.lock().clone();
        let path = url.split('?').next().unwrap_or(url);
        let rel = path
            .strip_prefix(&route.base)
            .unwrap_or(path)
            .trim_start_matches('/');
        // Browsers percent-encode every non-ASCII byte, so a page whose slug
        // carries one (`/posts/café/`) arrives as `%C3%A9` and matches no file
        // on disk. `within` still canonicalizes and containment-checks whatever
        // this produces, so decoding cannot open a traversal.
        let rel = Percent::decode(rel);
        let base = route.dist.join(&rel);
        self.within(&base)
            .or_else(|| self.within(&base.join("index.html")))
            .or_else(|| self.within(&route.dist.join(format!("{rel}.html"))))
    }

    /// The canonical path of `candidate` when it is an existing file inside
    /// `dist`, else `None`. The single guard against path traversal: both the
    /// root and the candidate are canonical, so `..` segments and symlinks that
    /// would leave the served tree are rejected before any read.
    fn within(&self, candidate: &Path) -> Option<PathBuf> {
        let canon = crate::fs::canonicalize(candidate).ok()?;
        (canon.starts_with(&self.route.lock().dist) && canon.is_file()).then_some(canon)
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

    /// Client script appended to served HTML: reloads on a successful rebuild,
    /// and overlays the diagnostic when one fails. The script is a lambda so the
    /// endpoint literal reaches it through the same `concat!` that keeps
    /// [`Live::ENDPOINT`] and the client in agreement.
    const SCRIPT: &'static str = concat!(
        "\n<script>\n(",
        include_str!("live.js"),
        ")('",
        live_endpoint!(),
        "');\n</script>\n"
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
}

impl Filter {
    fn new(config: &Config, root: &Root, config_path: &Path) -> Result<Self> {
        use notify::RecursiveMode::{NonRecursive, Recursive};
        let root = root.path().to_path_buf();
        let assets = Self::absolute(&root, &config.paths.assets);
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
        })
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
        crate::fs::canonicalize(path)
            .map(|p| p == self.config)
            .unwrap_or(path == self.config)
    }

    /// Whether a changed path should trigger a rebuild. Of `.kdl` files only
    /// the session's own config counts: baudelaire reads no other KDL, and
    /// the config directory's non-recursive watch also surfaces its siblings.
    fn is_relevant(&self, path: &Path) -> bool {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            config: config.clone(),
            ui: &ui,
            root: &root,
            config_path: PathBuf::from("config.kdl"),
            reload: Box::new(|| Ok(Config::default())),
        };
        dev.on_event(Err(vec![notify::Error::generic("boom")]), &live, &filter)
            .unwrap();
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
            config: config.clone(),
            ui: &ui,
            root: &root,
            config_path: PathBuf::from("config.kdl"),
            reload: Box::new(|| Ok(Config::default())),
        };
        dev.on_event(Ok(Vec::new()), &live, &filter).unwrap();
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
