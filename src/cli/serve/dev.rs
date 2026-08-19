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
    /// Re-reads `config.kdl` with the same profile and CLI overrides.
    pub(super) reload: Box<dyn FnMut() -> Result<Config> + 'a>,
    /// Directories the last build read outside the four source trees, watched
    /// on top of the defaults.
    pub(super) tracked: Vec<PathBuf>,
    /// Whether that set changed, so the watch loop re-registers with it.
    pub(super) rewatch: bool,
    /// The engine the last rebuild ran on, kept so the next one reuses the
    /// world it built: see [`Dev::rebuild`].
    pub(super) engine: Option<Engine>,
}

impl<'a> Dev<'a> {
    /// Start a session: build once, serve `dist`, and (unless `--no-watch`)
    /// watch for changes to rebuild and live-reload browsers. The CLI flags are
    /// already folded into `config.serve` by `ServeArgs::apply`.
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
            engine: None,
        }
        .run()
    }

    fn run(mut self) -> Result<()> {
        let requested = format!("{}:{}", self.config.serve.bind, self.config.serve.port);
        let server = Server::http(&requested).map_err(|e| ServeError::bind(&requested, e))?;
        let bound = server.server_addr().to_ip();
        let addr = bound.map_or(requested, |ip| ip.to_string());

        match self.rebuild() {
            Ok(stats) => self.tracked = stats.read,
            Err(e) => {
                self.ui.warn(RebuildFailed { errors: vec![e] });
                self.ui.flush();
            }
        }

        let watching = if self.config.serve.watch {
            Some(self.establish()?)
        } else {
            None
        };

        self.ui.blank();
        self.ui.arrow_kept(
            "local",
            format!("http://{addr}{}/", self.config.base_path())
                .cyan()
                .underline(),
        );
        self.ui.arrow(
            "watching",
            if self.config.serve.watch {
                crate::ui::Wrap::new(&self.watched(), crate::ui::ARROW_VALUE_COLUMN)
                    .to_string()
                    .dimmed()
                    .to_string()
            } else {
                "off (--no-watch)".dimmed().to_string()
            },
        );
        self.ui.blank();
        if self.config.serve.open {
            let url = format!("http://{addr}{}/", self.config.base_path());
            if let Err(e) = open::that_detached(&url) {
                self.ui.warn(BrowserOpen { url, source: e });
                self.ui.flush();
            }
        }

        let level = self.ui.level();
        let route = Arc::new(Mutex::new(Route::new(&self.config, bound)));
        if let Some(watching) = watching {
            let live = Live::default();
            Handler::new(Arc::clone(&route), Some(live.clone()), level).spawn(server);
            return self.watch(watching, &live, &route, bound);
        }
        Handler::new(route, None, level).serve(&server);
        Ok(())
    }

    /// Build the site once, on the engine the last rebuild used.
    ///
    /// The engine marks every file it loaded stale before each build, so a
    /// rebuild re-reads what changed and keeps typst's incremental state for
    /// everything it did not: the templates, the packages and the generated
    /// modules are parsed once per session rather than once per keystroke.
    ///
    /// A fresh one is built where the old one cannot answer for this build: a
    /// reloaded `config.kdl`, or build metadata the world fixed at
    /// construction and the machine has since moved past.
    fn rebuild(&mut self) -> Result<crate::engine::Stats> {
        let engine = match self.engine.take() {
            Some(engine) if engine.current() => engine,
            _ => Engine::new(self.config.clone(), Mode::Serve)?,
        };
        let built = engine.build(self.ui);
        self.engine = Some(engine);
        built
    }

    /// Drop the engine, so the next rebuild starts a world from this config.
    fn restart(&mut self) {
        self.engine = None;
    }

    /// The watched roots, for the startup banner: the defaults, the config
    /// file, plus any `serve.include` globs. Returned as separate items so the
    /// banner can wrap them to the terminal width.
    fn watched(&self) -> Vec<String> {
        let mut parts: Vec<String> = Filter::registered(&self.config, self.root)
            .iter()
            .map(|dir| dir.display().to_string())
            .collect();
        parts.push(self.config_path.display().to_string());
        parts.extend(self.config.serve.include.iter().cloned());
        parts
    }

    /// Register the watcher and open the channel its events arrive on, before
    /// the session is announced: a file event is edge-triggered, so an edit
    /// saved before registration would reach nobody. Events arriving before the
    /// loop consumes them are not lost, the channel being unbounded.
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
    /// `watching` is re-established whenever `config.kdl` is reloaded, though a
    /// `bind`/`port` change still needs a restart: the server is already bound.
    fn watch(
        mut self,
        mut watching: Watching,
        live: &Live,
        route: &Mutex<Route>,
        bound: Option<std::net::SocketAddr>,
    ) -> Result<()> {
        loop {
            self.rewatch = false;
            let mut reloaded = false;
            for result in &watching.rx {
                let outcome = self.on_event(result, live, &watching.filter);
                self.ui.flush();
                if outcome || self.rewatch {
                    reloaded = true;
                    break;
                }
            }
            if !reloaded {
                return Ok(());
            }
            *route.lock() = Route::new(&self.config, bound);
            watching = self.establish()?;
        }
    }

    /// Handle one debounced watcher delivery, surfacing watcher failures as
    /// warnings. Returns whether `config.kdl` was reloaded, so the caller
    /// recreates the watcher.
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

        let config_changed = changed.iter().any(|p| filter.is_config(p));
        if config_changed {
            match (self.reload)() {
                Ok(config) => {
                    self.config = config;
                    self.restart();
                }
                Err(e) => {
                    self.ui.warn(ConfigReload { errors: vec![e] });
                    return false;
                }
            }
        }

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
                self.ui
                    .event(label, stats.pages - stats.cached, timer.elapsed());
                live.bump();
                if stats.read != self.tracked {
                    self.tracked = stats.read;
                    self.rewatch = true;
                }
            }
            Err(e) => {
                let failure = RebuildFailed { errors: vec![e] };
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

    /// Whether an event actually changes content. Excludes `Access` and
    /// metadata events: a rebuild reads every source, and reacting to those
    /// reads would loop the watcher forever.
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
