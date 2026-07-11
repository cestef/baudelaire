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
use crate::cli::output::{Level, Paths, Report};
use crate::config::Config;
use crate::engine::{Engine, Mode};
use crate::error::{ContentError, Result};
use crate::error::serve::ServeError;
use crate::mime::Mime;

/// Run the dev server: build once, serve `dist`, and (unless `--no-watch`)
/// watch for changes to rebuild and live-reload browsers.
///
/// CLI flags (`--port`, `--bind`, `--open`, `--no-watch`) are already folded
/// into `config.serve` by [`crate::cli::ServeArgs::apply`].
pub(crate) fn run(report: &mut Report, config: &Config, root: &Root) -> Result<()> {
    Dev { config, report, root }.run()
}

/// Orchestrates a dev-server session: the initial build, the HTTP handler, and
/// the watch/rebuild loop.
struct Dev<'a> {
    config: &'a Config,
    report: &'a mut Report,
    root: &'a Root,
}

impl Dev<'_> {
    fn run(self) -> Result<()> {
        let addr = format!("{}:{}", self.config.serve.bind, self.config.serve.port);
        let server = Server::http(&addr).map_err(|e| ServeError::bind(&addr, e))?;

        Engine::new(self.config.clone(), Mode::Serve)?.build(self.report)?;
        self.report.milestone(format_args!(
            "serving {} at {}",
            self.config.label(),
            format!("http://{addr}").cyan().underline()
        ))?;
        if self.config.serve.open {
            // Detached: `open::that` waits for the spawned program to exit, so a
            // browser launched in the foreground would block the watch loop until
            // its window closed. Failing to open a browser is non-fatal — the
            // server is already up — so report it and carry on.
            if let Err(e) = open::that_detached(format!("http://{addr}")) {
                self.report.warn(format_args!("could not open browser: {e}"))?;
            }
        }

        let level = self.report.level();
        if self.config.serve.watch {
            let live = Live::default();
            Handler::new(self.config.dist.clone(), Some(live.clone()), level).spawn(server);
            self.watch(live)
        } else {
            Handler::new(self.config.dist.clone(), None, level).serve(&server);
            Ok(())
        }
    }

    /// Watch content, templates, assets, and any `include` globs, rebuilding on
    /// every relevant change.
    fn watch(mut self, live: Live) -> Result<()> {
        let (tx, rx) = flume::unbounded::<DebounceEventResult>();
        let filter = Filter::new(self.config, self.root)?;
        let _watcher = Watcher::new(filter.dirs(), tx)?;
        self.report.muted("watching for changes")?;
        for result in rx {
            self.on_event(result, &live, &filter)?;
        }
        Ok(())
    }

    /// Handle one debounced watcher delivery: rebuild on events, and surface
    /// watcher failures (dropped watches, queue overflow) as warnings instead
    /// of silently discarding them — the server keeps serving either way.
    fn on_event(&mut self, result: DebounceEventResult, live: &Live, filter: &Filter) -> Result<()> {
        match result {
            Ok(events) => self.on_change(events, live, filter),
            Err(errors) => {
                for error in errors {
                    self.report.warn(format_args!("file watcher error: {error}"))?;
                }
                Ok(())
            }
        }
    }

    /// Rebuild after a batch of file events, then push a live reload on success.
    fn on_change(
        &mut self,
        events: Vec<DebouncedEvent>,
        live: &Live,
        filter: &Filter,
    ) -> Result<()> {
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
            return Ok(());
        }

        // A Vite-style rebuild: a transient status while the build runs, replaced
        // by a single timestamped log line. The build's own milestone/summary is
        // silenced so rebuilds never stack the full block over the initial output.
        let label = Self::label(&changed, self.root);
        self.report.status(format_args!("rebuilding {}", Paths(&label)))?;
        let prior = self.report.level();
        self.report.set_level(Level::Silent);
        let result = Engine::new(self.config.clone(), Mode::Serve).and_then(|e| e.build(self.report));
        self.report.set_level(prior);

        match result {
            Ok(stats) => {
                // Report what the rebuild actually recompiled, not the whole site.
                self.report.event(label, stats.pages - stats.cached)?;
                live.bump();
            }
            Err(e) => {
                // Render the full diagnostic — spans, offending page, related
                // errors — the way the top-level miette handler would, instead
                // of collapsing it to a one-line `Display`.
                self.report.warn("rebuild failed")?;
                eprintln!("{:?}", miette::Report::from(e));
            }
        }
        Ok(())
    }

    /// A concise label for a rebuild's trigger: the first changed file (relative
    /// to the project root) and, when several changed, how many more.
    fn label(changed: &[&PathBuf], root: &Root) -> String {
        let first = changed[0].strip_prefix(root.path()).unwrap_or(changed[0]).display();
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

/// Serves files from `dist`, optionally injecting live reload. Cloned into the
/// request-handling thread, so it is cheap and `Send`.
#[derive(Clone)]
struct Handler {
    dist: PathBuf,
    live: Option<Live>,
    /// The session's verbosity, copied in at construction so per-request
    /// logging (404s) honors `--quiet` like every other line.
    level: Level,
}

impl Handler {
    fn new(dist: PathBuf, live: Option<Live>, level: Level) -> Self {
        // Canonicalize the served root up front so every per-request traversal
        // check compares canonical paths (with `..` and symlinks resolved)
        // against a canonical root.
        let dist = crate::fs::canonicalize(&dist).unwrap_or(dist);
        Self { dist, live, level }
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
            Some(file) => self.serve_file(req, &file),
            None => self.respond_404(req, &url),
        }
    }

    /// Serve a file with content-type guessing, injecting the live-reload
    /// client into HTML when live reload is enabled.
    fn serve_file(&self, req: Request, path: &Path) {
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
        let mut response = Response::from_data(body);
        if let Ok(header) = Header::from_bytes(b"Content-Type", mime.header().as_bytes()) {
            response = response.with_header(header);
        }
        let _ = req.respond(response);
    }

    fn respond_404(&self, req: Request, url: &str) {
        let _ = req.respond(Response::empty(404));
        // A per-request report at the session's level, so `--quiet` silences
        // these lines too. (It may still interleave with a concurrent rebuild
        // status line; acceptable for now.)
        let _ = Report::with_level(self.level).muted(format_args!("  {} {}", "✗".red(), url.dimmed()));
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
/// dead socket on the failed write, and removes its own entry — no leak waiting
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

/// Debounced file watcher for the content + templates directories.
struct Watcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

impl Watcher {
    fn new(dirs: &[PathBuf], tx: flume::Sender<DebounceEventResult>) -> Result<Self> {
        let handler = move |result: DebounceEventResult| {
            let _ = tx.send(result);
        };
        let mut debouncer = new_debouncer(Duration::from_millis(500), None, handler)
            .map_err(ServeError::watcher_init)?;
        for dir in dirs {
            if dir.exists() {
                debouncer
                    .watch(dir, notify::RecursiveMode::Recursive)
                    .map_err(|e| ServeError::watch(dir, e))?;
            }
        }
        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

/// Decides which changed paths trigger a rebuild, and which directories to
/// watch. Built once per session from `serve.include` / `serve.exclude` wax
/// globs: `exclude` (e.g. hook-generated files) wins over everything, then
/// `include` adds sources beyond the defaults (content, templates, assets).
struct Filter {
    root: PathBuf,
    assets: PathBuf,
    dirs: Vec<PathBuf>,
    include: Vec<Glob<'static>>,
    exclude: Vec<Glob<'static>>,
}

impl Filter {
    fn new(config: &Config, root: &Root) -> Result<Self> {
        let root = root.path().to_path_buf();
        let assets =
            crate::fs::canonicalize(&config.assets).unwrap_or_else(|_| root.join(&config.assets));
        let mut dirs = vec![
            config.content.clone(),
            config.templates.clone(),
            config.assets.clone(),
        ];
        let include = Self::compile(&config.serve.include)?;
        // Watch the literal prefix directory of each include glob (e.g. `data/`
        // in `data/**/*.json`) so its files are actually observed.
        for glob in &include {
            let (prefix, _) = glob.clone().partition();
            if !prefix.as_os_str().is_empty() {
                dirs.push(root.join(prefix));
            }
        }
        let exclude = Self::compile(&config.serve.exclude)?;
        Ok(Self {
            root,
            assets,
            dirs,
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

    fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// Whether a changed path should trigger a rebuild.
    fn is_relevant(&self, path: &Path) -> bool {
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        if self.exclude.iter().any(|g| g.is_match(rel)) {
            return false;
        }
        if self.include.iter().any(|g| g.is_match(rel)) {
            return true;
        }
        path.extension().is_some_and(|e| e == "typ" || e == "kdl") || path.starts_with(&self.assets)
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
        let mut report = Report::with_level(Level::Silent);
        let filter = Filter::new(&config, &root).unwrap();
        let live = Live::default();
        let mut dev = Dev {
            config: &config,
            report: &mut report,
            root: &root,
        };
        dev.on_event(Err(vec![notify::Error::generic("boom")]), &live, &filter)
            .unwrap();
        assert_eq!(dev.report.warnings(), 1);
    }

    /// The `Ok` arm still flows into change handling: irrelevant (empty) event
    /// batches are a no-op and produce no warnings.
    #[test]
    fn empty_event_batch_is_a_no_op() {
        let config = Config::default();
        let root = Root::at(".");
        let mut report = Report::with_level(Level::Silent);
        let filter = Filter::new(&config, &root).unwrap();
        let live = Live::default();
        let mut dev = Dev {
            config: &config,
            report: &mut report,
            root: &root,
        };
        dev.on_event(Ok(Vec::new()), &live, &filter).unwrap();
        assert_eq!(dev.report.warnings(), 0);
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
