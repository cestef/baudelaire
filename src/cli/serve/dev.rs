//! One dev-server session: build, serve, watch, rebuild, reload.

use std::path::PathBuf;
use std::sync::Arc;

use itertools::Itertools;
use notify_debouncer_full::{DebounceEventResult, DebouncedEvent};
use owo_colors::OwoColorize;
use parking_lot::Mutex;
use tiny_http::Server;

use super::live::Live;
use super::route::{Handler, Route};
use super::watch::{Filter, Watcher, Watching};
use crate::cli::Root;
use crate::config::Config;
use crate::engine::{Engine, Mode};
use crate::error::Result;
use crate::error::serve::ServeError;
use crate::error::warning::{BrowserOpen, ConfigReload, RebuildFailed, WatchLost};
use crate::ui::{Level, Paths, Timer, Ui};

/// Orchestrates a dev-server session: the initial build, the HTTP handler, and
/// the watch/rebuild loop. Owns its [`Config`] so a change to `config.kdl` can
/// reload it (via [`Dev::reload`]) without restarting the process.
pub(super) struct Dev<'a> {
    pub(super) config: Config,
    pub(super) ui: &'a Ui,
    pub(super) root: &'a Root,
    /// The config file the session was started with (`--config`), watched so
    /// edits to it reload the session live.
    pub(super) config_path: PathBuf,
    /// Re-reads `config.kdl` with the same profile and CLI overrides, invoked
    /// when the config file changes so edits take effect live.
    pub(super) reload: Box<dyn FnMut() -> Result<Config> + 'a>,
    /// Directories the last build read outside the four source trees: a `data/`
    /// tree a page loaded, wherever the site keeps it. Watched on top of the
    /// defaults, so an input the build demonstrably depends on does not also
    /// have to be named in `serve { include }`.
    pub(super) tracked: Vec<PathBuf>,
    /// Whether that set changed, so the watch loop re-registers with it.
    pub(super) rewatch: bool,
}

impl<'a> Dev<'a> {
    /// Start a session: build once, serve `dist`, and (unless `--no-watch`)
    /// watch for changes to rebuild and live-reload browsers.
    ///
    /// The session's own constructor, so the fields below stay private to it:
    /// assembling them was a free function in the parent module, which is the
    /// one place outside this file that had to know what a session is made of.
    /// CLI flags (`--port`, `--bind`, `--open`, `--no-watch`) are already folded
    /// into `config.serve` by `ServeArgs::apply`.
    pub(super) fn start(
        ui: &'a Ui,
        config: Config,
        root: &'a Root,
        config_path: PathBuf,
        reload: impl FnMut() -> Result<Config> + 'a,
    ) -> Result<()> {
        Self {
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
                true => crate::ui::Wrap::new(&self.watched(), crate::ui::ARROW_VALUE_COLUMN)
                    .to_string()
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
        // a directory nothing watches: one that is not on disk is skipped by
        // both, through the same predicate.
        let mut parts: Vec<String> = Filter::registered(&self.config, self.root)
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
    pub(super) fn on_event(
        &mut self,
        result: DebounceEventResult,
        live: &Live,
        filter: &Filter,
    ) -> bool {
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
