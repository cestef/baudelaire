//! Watching the sources: which paths matter, and what a change means.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use wax::{Glob, Program};

use crate::cli::Root;
use crate::config::Config;
use crate::error::serve::ServeError;
use crate::error::{ContentError, Result};

/// Debounced file watcher over the session's watch roots.
pub(super) struct Watcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

/// A live registration: the watcher, what it decided to watch, and the channel
/// its debounced events arrive on. The three travel together because they are
/// only meaningful together, and because dropping the [`Watcher`] is what
/// unregisters it.
pub(super) struct Watching {
    pub(super) filter: Filter,
    pub(super) rx: flume::Receiver<DebounceEventResult>,
    pub(super) _watcher: Watcher,
}

impl Watcher {
    pub(super) fn new(
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
pub(super) struct Filter {
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
    pub(super) fn new(config: &Config, root: &Root, config_path: &Path) -> Result<Self> {
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
    pub(super) fn watching(mut self, dirs: &[PathBuf]) -> Self {
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
    pub(super) fn roots(config: &Config) -> [&Path; 4] {
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

    pub(super) fn watches(&self) -> &[(PathBuf, notify::RecursiveMode)] {
        &self.watches
    }

    /// Whether a changed path is the session's config file. Canonicalized when
    /// possible so a symlinked event path still matches; falls back to a raw
    /// compare when the file is mid-rename (deleted, about to reappear).
    pub(super) fn is_config(&self, path: &Path) -> bool {
        crate::fs::canonicalize(path).map_or(path == self.config, |p| p == self.config)
    }

    /// Whether a changed path should trigger a rebuild. Of `.kdl` files only
    /// the session's own config counts: baudelaire reads no other KDL, and
    /// the config directory's non-recursive watch also surfaces its siblings.
    pub(super) fn is_relevant(&self, path: &Path) -> bool {
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
    use crate::cli::serve::dev::Dev;
    use crate::cli::serve::live::Live;
    use crate::ui::{Level, Ui};

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
}
