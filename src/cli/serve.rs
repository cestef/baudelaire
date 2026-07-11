//! Dev server: serve the built site, watch for changes, rebuild and live-reload.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use itertools::Itertools;
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};
use owo_colors::OwoColorize;
use tiny_http::{Header, Request, Response, Server};
use wax::{Glob, Program};

use crate::cli::output::{Level, Paths, Report};
use crate::config::Config;
use crate::engine::{Engine, Mode};
use crate::error::{ContentError, Result};
use crate::error::serve::ServeError;

/// Run the dev server: build once, serve `dist`, and (unless `--no-watch`)
/// watch for changes to rebuild and live-reload browsers.
///
/// CLI flags (`--port`, `--bind`, `--open`, `--no-watch`) are already folded
/// into `config.serve` by [`crate::cli::ServeArgs::apply`].
pub fn run(report: &mut Report, config: &Config) -> Result<()> {
    Dev { config, report }.run()
}

/// Orchestrates a dev-server session: the initial build, the HTTP handler, and
/// the watch/rebuild loop.
struct Dev<'a> {
    config: &'a Config,
    report: &'a mut Report,
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

        if self.config.serve.watch {
            let live = Live::default();
            Handler::new(self.config.dist.clone(), Some(live.clone())).spawn(server);
            self.watch(live)
        } else {
            Handler::new(self.config.dist.clone(), None).serve(&server);
            Ok(())
        }
    }

    /// Watch content, templates, assets, and any `include` globs, rebuilding on
    /// every relevant change.
    fn watch(mut self, live: Live) -> Result<()> {
        let (tx, rx) = mpsc::channel::<DebounceEventResult>();
        let filter = Filter::new(self.config)?;
        let _watcher = Watcher::new(filter.dirs(), tx)?;
        self.report.muted("watching for changes")?;
        for events in rx.into_iter().flatten() {
            self.on_change(events, &live, &filter)?;
        }
        Ok(())
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
        let label = Self::label(&changed);
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
    fn label(changed: &[&PathBuf]) -> String {
        let cwd = std::env::current_dir().unwrap_or_default();
        let first = changed[0].strip_prefix(&cwd).unwrap_or(changed[0]).display();
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
}

impl Handler {
    fn new(dist: PathBuf, live: Option<Live>) -> Self {
        Self { dist, live }
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
            Some(file) if file.exists() => self.serve_file(req, &file),
            _ => Self::respond_404(req, &url),
        }
    }

    /// Serve a file with content-type guessing, injecting the live-reload
    /// client into HTML when live reload is enabled.
    fn serve_file(&self, req: Request, path: &Path) {
        let content_type = Self::mime(path);
        let mut body = std::fs::read(path).unwrap_or_default();
        if self.live.is_some() && content_type.starts_with("text/html") {
            body.extend_from_slice(Live::SCRIPT.as_bytes());
        }
        let mut response = Response::from_data(body);
        if let Ok(header) = Header::from_bytes(b"Content-Type", content_type.as_bytes()) {
            response = response.with_header(header);
        }
        let _ = req.respond(response);
    }

    fn respond_404(req: Request, url: &str) {
        let _ = req.respond(Response::empty(404));
        let _ = Report::stdout().muted(format_args!("  {} {}", "✗".red(), url.dimmed()));
    }

    /// Resolve a URL path to a file under `dist`, honoring clean URLs.
    fn resolve(&self, url: &str) -> Option<PathBuf> {
        let rel = url.split('?').next().unwrap_or(url).trim_start_matches('/');
        let base = self.dist.join(rel);
        if base.is_file() {
            return Some(base);
        }
        let index = base.join("index.html");
        if index.is_file() {
            return Some(index);
        }
        Some(self.dist.join(format!("{rel}.html")))
    }

    /// Guess a content type from the file extension.
    fn mime(path: &Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("js") => "application/javascript",
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("svg") => "image/svg+xml",
            Some("json") => "application/json",
            Some("woff2") => "font/woff2",
            _ => "application/octet-stream",
        }
    }
}

/// Live-reload coordination between the request handler and the rebuild loop.
///
/// The handler injects [`Live::SCRIPT`] into HTML responses; the injected
/// client opens a Server-Sent Events stream at [`Live::ENDPOINT`]. Each
/// successful rebuild calls [`Live::bump`], pushing a reload to every open
/// stream.
#[derive(Clone, Default)]
struct Live {
    /// One sender per open SSE connection.
    streams: Arc<Mutex<Vec<mpsc::Sender<()>>>>,
}

impl Live {
    /// Endpoint the injected client connects to for the reload event stream.
    const ENDPOINT: &'static str = "/__baudelaire/live";

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

    /// Advance every open stream, dropping any that have closed.
    fn bump(&self) {
        self.streams
            .lock()
            .expect("lock")
            .retain(|tx| tx.send(()).is_ok());
    }

    /// Open an SSE stream for `req` on its own thread, writing directly to the
    /// socket so each event flushes the instant a rebuild finishes.
    fn serve(&self, req: Request) {
        let (tx, signals) = mpsc::channel();
        self.streams.lock().expect("lock").push(tx);
        std::thread::spawn(move || {
            let mut socket = req.into_writer();
            if socket.write_all(Self::HEAD.as_bytes()).is_err() || socket.flush().is_err() {
                return;
            }
            while signals.recv().is_ok() {
                if socket.write_all(b"data: reload\n\n").is_err() || socket.flush().is_err() {
                    break;
                }
            }
        });
    }
}

/// Debounced file watcher for the content + templates directories.
struct Watcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

impl Watcher {
    fn new(dirs: &[PathBuf], tx: mpsc::Sender<DebounceEventResult>) -> Result<Self> {
        let mut debouncer =
            new_debouncer(Duration::from_millis(500), None, tx).map_err(ServeError::watcher)?;
        for dir in dirs {
            if dir.exists() {
                debouncer
                    .watch(dir, notify::RecursiveMode::Recursive)
                    .map_err(ServeError::watcher)?;
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
    fn new(config: &Config) -> Result<Self> {
        let root = std::env::current_dir().unwrap_or_default();
        let assets = config
            .assets
            .canonicalize()
            .unwrap_or_else(|_| root.join(&config.assets));
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
