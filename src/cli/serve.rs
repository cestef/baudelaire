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
use crate::config::Config;
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
    fn run(self) -> Result<()> {
        let addr = format!("{}:{}", self.config.serve.bind, self.config.serve.port);
        let server = Server::http(&addr).map_err(|e| ServeError::bind(&addr, e))?;

        Engine::new(self.config.clone(), Mode::Serve)?.build(self.ui)?;
        self.ui.blank();
        self.ui
            .arrow("local", format!("http://{addr}/").cyan().underline());
        self.ui.arrow(
            "watching",
            match self.config.serve.watch {
                true => self.watched().dimmed().to_string(),
                false => "off (--no-watch)".dimmed().to_string(),
            },
        );
        self.ui.blank();
        if self.config.serve.open {
            // Detached: `open::that` waits for the spawned program to exit, so a
            // browser launched in the foreground would block the watch loop until
            // its window closed. Failing to open a browser is non-fatal (the
            // server is already up), so report it and carry on.
            let url = format!("http://{addr}/");
            if let Err(e) = open::that_detached(&url) {
                self.ui.warn(BrowserOpen { url, source: e });
                self.ui.flush();
            }
        }

        let level = self.ui.level();
        if self.config.serve.watch {
            let live = Live::default();
            Handler::new(self.config.dist.clone(), Some(live.clone()), level).spawn(server);
            self.watch(live)
        } else {
            Handler::new(self.config.dist.clone(), None, level).serve(&server);
            Ok(())
        }
    }

    /// The watched roots, for the startup banner: the defaults, the config
    /// file, plus any `serve.include` globs.
    fn watched(&self) -> String {
        let mut parts = vec![
            self.config.content.display().to_string(),
            self.config.templates.display().to_string(),
            self.config.assets.display().to_string(),
            self.config.r#static.display().to_string(),
            self.config_path.display().to_string(),
        ];
        parts.extend(self.config.serve.include.iter().cloned());
        parts.join(" · ")
    }

    /// Watch content, templates, assets, and any `include` globs, rebuilding on
    /// every relevant change.
    fn watch(mut self, live: Live) -> Result<()> {
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
        let result = Engine::new(self.config.clone(), Mode::Serve).and_then(|e| e.build(self.ui));
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
                // offending page and all), rendered by the caller's flush.
                self.ui.warn(RebuildFailed { errors: vec![e] });
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
    dist: PathBuf,
    live: Option<Live>,
    /// The handler's own [`Ui`] at the session's verbosity, so per-request
    /// logging (404s) honors `--quiet` like every other line without sharing
    /// the rebuild loop's writer.
    ui: Ui,
}

impl Handler {
    fn new(dist: PathBuf, live: Option<Live>, level: Level) -> Self {
        // Canonicalize the served root up front so every per-request traversal
        // check compares canonical paths (with `..` and symlinks resolved)
        // against a canonical root.
        let dist = crate::fs::canonicalize(&dist).unwrap_or(dist);
        Self {
            dist,
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
        match self.within(&self.dist.join(crate::config::Config::NOT_FOUND)) {
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
        let rel = url.split('?').next().unwrap_or(url).trim_start_matches('/');
        let base = self.dist.join(rel);
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
    streams: Arc<Mutex<HashMap<u64, flume::Sender<()>>>>,
    /// Monotonic source of stream ids.
    next_id: Arc<AtomicU64>,
}

impl Live {
    /// Endpoint the injected client connects to for the reload event stream.
    const ENDPOINT: &'static str = "/__baudelaire/live";

    /// How often an idle stream emits a keep-alive comment. Doubles as the upper
    /// bound on how long a closed connection lingers before it is reaped.
    const HEARTBEAT: Duration = Duration::from_secs(10);

    /// Client script appended to served HTML; reloads on each pushed event.
    const SCRIPT: &'static str = "\n<script>\n\
        new EventSource('/__baudelaire/live').onmessage = function () { location.reload(); };\
        \n</script>\n";

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
        self.streams.lock().retain(|_, tx| tx.send(()).is_ok());
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
                        Ok(()) => "data: reload\n\n",
                        Err(flume::RecvTimeoutError::Timeout) => ": ping\n\n",
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
        let assets =
            crate::fs::canonicalize(&config.assets).unwrap_or_else(|_| root.join(&config.assets));
        let statics = crate::fs::canonicalize(&config.r#static)
            .unwrap_or_else(|_| root.join(&config.r#static));
        let mut watches = vec![
            (config.content.clone(), Recursive),
            (config.templates.clone(), Recursive),
            (config.assets.clone(), Recursive),
            (config.r#static.clone(), Recursive),
        ];
        // Watch the config file via its parent directory, non-recursively:
        // editors commonly save by rename-over, which drops a watch pinned to
        // the file itself. A bare `config.kdl` has an empty parent, meaning
        // the project root.
        let config_dir = match config_path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
            _ => root.clone(),
        };
        watches.push((config_dir, NonRecursive));
        // Absolute identity for the config file: canonical when it exists,
        // else root-resolved, so event paths (absolute) compare against it.
        let config_file = crate::fs::canonicalize(config_path).unwrap_or_else(|_| {
            if config_path.is_absolute() {
                config_path.to_path_buf()
            } else {
                root.join(config_path)
            }
        });
        let include = Self::compile(&config.serve.include)?;
        // Watch the literal prefix directory of each include glob (e.g. `data/`
        // in `data/**/*.json`) so its files are actually observed.
        for glob in &include {
            let (prefix, _) = glob.clone().partition();
            if !prefix.as_os_str().is_empty() {
                watches.push((root.join(prefix), Recursive));
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

    /// Compile a list of patterns into owned globs.
    fn compile(patterns: &[String]) -> Result<Vec<Glob<'static>>> {
        patterns
            .iter()
            .map(|pattern| {
                Glob::new(pattern)
                    .map(Glob::into_owned)
                    .map_err(|e| ContentError::bad_glob(pattern, e).into())
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
        let (dead_tx, dead_rx) = flume::unbounded::<()>();
        live.streams.lock().insert(0, live_tx);
        live.streams.lock().insert(1, dead_tx);
        // The dead stream's receiver (its writer thread) is gone.
        drop(dead_rx);

        live.bump();

        let streams = live.streams.lock();
        assert!(streams.contains_key(&0), "live stream kept");
        assert!(!streams.contains_key(&1), "disconnected stream reaped");
        // The surviving stream received the reload signal.
        assert!(live_rx.try_recv().is_ok());
    }
}
