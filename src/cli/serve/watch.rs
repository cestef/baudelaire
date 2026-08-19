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
/// its debounced events arrive on. Dropping the [`Watcher`] unregisters it.
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
            if Filter::watchable(dir) {
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
/// `serve.exclude` wins over everything, then `serve.include` adds sources
/// beyond the defaults (content, templates, assets, and the config file).
pub(super) struct Filter {
    root: PathBuf,
    /// The source trees, resolved: membership of these is what makes a change
    /// relevant, never the file's extension.
    trees: [PathBuf; 4],
    /// The session's config file, absolute, so a sibling `.kdl` in the same
    /// directory does not reload the session.
    config: PathBuf,
    watches: Vec<(PathBuf, notify::RecursiveMode)>,
    include: Vec<Glob<'static>>,
    exclude: Vec<Glob<'static>>,
    /// The files `paths { sources { } }` declares, absolute: the only build
    /// input that may sit outside the project root entirely.
    sourced: Vec<PathBuf>,
    /// Directories the last build read outside the watched trees (see
    /// [`Filter::watching`]), so an event in one is relevant without a glob.
    tracked: Vec<PathBuf>,
    /// The build's whole scratch tree, which is never an input however much it
    /// looks like one.
    scratch: PathBuf,
    /// The output directory, never an input for the same reason as `scratch`.
    dist: PathBuf,
}

impl Filter {
    /// The config file and each declared source are watched through their
    /// parent directory, non-recursively: an editor saving by rename-over drops
    /// a watch pinned to the file itself.
    pub(super) fn new(config: &Config, root: &Root, config_path: &Path) -> Result<Self> {
        use notify::RecursiveMode::{NonRecursive, Recursive};
        let base = root.path().to_path_buf();
        let cache = Self::absolute(&base, Path::new(crate::config::Config::SCRATCH));
        let trees = Self::roots(config).map(|dir| Self::absolute(&base, dir));
        let mut watches: Vec<(PathBuf, notify::RecursiveMode)> = Vec::new();
        for dir in &trees {
            Self::observe(&mut watches, dir.clone(), Recursive);
        }
        let config_dir = match config_path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => Self::absolute(&base, dir),
            _ => base.clone(),
        };
        Self::observe(&mut watches, config_dir, NonRecursive);
        let config_file = Self::absolute(&base, config_path);
        let include = Self::compile(&config.serve.include)?;
        for glob in &include {
            let (prefix, _) = glob.clone().partition();
            if !prefix.as_os_str().is_empty() {
                Self::observe(&mut watches, Self::absolute(&base, &prefix), Recursive);
            }
        }
        let sourced: Vec<PathBuf> = config
            .paths
            .sources
            .iter()
            .map(|(_, path)| Self::absolute(&base, path))
            .collect();
        for file in &sourced {
            if let Some(dir) = file.parent() {
                Self::observe(&mut watches, dir.to_path_buf(), NonRecursive);
            }
        }
        let exclude = Self::compile(&config.serve.exclude)?;
        let dist = Self::absolute(&base, &config.paths.dist);
        Ok(Self {
            root: base,
            trees,
            config: config_file,
            watches,
            include,
            exclude,
            sourced,
            tracked: Vec::new(),
            scratch: cache,
            dist,
        })
    }

    /// Also watch `dirs`, the directories the last build read outside those
    /// trees, and treat what they hold as relevant. Additive: `serve { include }`
    /// still covers what no compile reads.
    pub(super) fn watching(mut self, dirs: &[PathBuf]) -> Self {
        for dir in dirs {
            Self::observe(
                &mut self.watches,
                dir.clone(),
                notify::RecursiveMode::Recursive,
            );
        }
        self.tracked = dirs.to_vec();
        self
    }

    /// Record one directory to watch, keeping the deeper mode when it is
    /// already recorded: notify's fsevent backend keys its watches by path, so
    /// a later non-recursive registration would overwrite a recursive one.
    fn observe(
        watches: &mut Vec<(PathBuf, notify::RecursiveMode)>,
        dir: PathBuf,
        mode: notify::RecursiveMode,
    ) {
        match watches.iter_mut().find(|(at, _)| *at == dir) {
            Some((_, recorded)) => {
                if mode == notify::RecursiveMode::Recursive {
                    *recorded = mode;
                }
            }
            None => watches.push((dir, mode)),
        }
    }

    /// The source trees a session always watches, in the configured (relative)
    /// spelling, derived from [`Paths::trees`].
    pub(super) fn roots(config: &Config) -> [&Path; 4] {
        config.paths.trees().map(|(_, dir)| dir)
    }

    /// Whether a directory can be watched at all: notify refuses one that is
    /// not there.
    fn watchable(dir: &Path) -> bool {
        dir.exists()
    }

    /// The source trees actually registered, in the relative spelling the
    /// banner prints: [`Filter::roots`] minus the ones that are not on disk.
    pub(super) fn registered<'a>(config: &'a Config, root: &Root) -> Vec<&'a Path> {
        Self::roots(config)
            .into_iter()
            .filter(|dir| Self::watchable(&root.join(dir)))
            .collect()
    }

    /// A path in the one form this filter compares in: absolute, and canonical
    /// as far as it exists. Watch roots go through here too, since a watcher
    /// reports events under the path it was registered with. `resolved`, not
    /// `canonical`, so a path that does not exist yet spells the same as it
    /// will once it appears.
    fn absolute(root: &Path, path: &Path) -> PathBuf {
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        crate::fs::resolved(joined)
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
        crate::fs::canonical(path) == self.config
    }

    /// Whether a changed path should trigger a rebuild: anything inside one of
    /// the watched source trees, anything the last build read outside them, and
    /// the session's own config file. Membership of a tree, never a file
    /// extension; nothing the build itself writes counts, or every build would
    /// queue the next one.
    pub(super) fn is_relevant(&self, path: &Path) -> bool {
        if path.starts_with(&self.scratch) || path.starts_with(&self.dist) {
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
            || self.sourced.iter().any(|file| file == path)
            || self.trees.iter().any(|tree| path.starts_with(tree))
            || self.tracked.iter().any(|dir| path.starts_with(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::serve::dev::Dev;
    use crate::cli::serve::live::Live;
    use crate::ui::{Level, Ui};

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
            engine: None,
        };
        dev.on_event(Err(vec![notify::Error::generic("boom")]), &live, &filter);
        assert_eq!(ui.warnings(), 1);
    }

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
            engine: None,
        };
        dev.on_event(Ok(Vec::new()), &live, &filter);
        assert_eq!(ui.warnings(), 0);
    }

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
        assert!(!filter.is_relevant(Path::new("/proj/README.md")));
        assert!(!filter.is_relevant(Path::new("/proj/other.kdl")));
        assert!(filter.is_config(Path::new("/proj/config.kdl")));
        assert!(!filter.is_config(Path::new("/proj/other.kdl")));
    }

    #[test]
    fn a_directory_watched_twice_keeps_the_deeper_mode() {
        let mut config = Config::default();
        config.paths.sources = vec![("notes".to_owned(), PathBuf::from("content/notes/n.typ"))];
        let root = Root::at("/proj");
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();

        let content: Vec<_> = filter
            .watches()
            .iter()
            .filter(|(dir, _)| dir == Path::new("/proj/content"))
            .collect();
        assert_eq!(content.len(), 1, "{:?}", filter.watches());
        assert_eq!(
            content[0].1,
            notify::RecursiveMode::Recursive,
            "the tree's own mode has to win: {:?}",
            filter.watches()
        );
        assert!(
            filter
                .watches()
                .iter()
                .any(|(dir, _)| dir == Path::new("/proj/content/notes")),
            "{:?}",
            filter.watches()
        );
    }

    #[test]
    fn a_declared_source_spells_the_same_before_and_after_it_appears() {
        let tmp = tempfile::tempdir().unwrap();
        let base = crate::fs::canonical(tmp.path());
        crate::fs::create_dir_all(base.join("site")).unwrap();

        let mut config = Config::default();
        config.paths.sources = vec![("notes".to_owned(), PathBuf::from("../notes.md"))];
        let root = Root::at(base.join("site"));
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();

        let expected = base.join("notes.md");
        assert!(
            filter.is_relevant(&expected),
            "a source not yet written is judged against {expected:?}: {:?}",
            filter.sourced
        );

        std::fs::write(&expected, "prose").unwrap();
        assert!(filter.is_relevant(&crate::fs::canonical(&expected)));
    }

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
    #[test]
    fn anything_inside_a_source_tree_is_relevant_whatever_it_is_called() {
        let config = Config::default();
        let root = Root::at("/proj");
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();
        let content = Path::new("/proj").join(&config.paths.content);
        let templates = Path::new("/proj").join(&config.paths.templates);
        for path in [
            content.join("posts/a.typ"),
            content.join("posts/a.md"),
            content.join("posts/a/index.md"),
            content.join("posts/a/data.json"),
            content.join("posts/a/photo.png"),
            templates.join("layout.typ"),
            templates.join("authors.yaml"),
        ] {
            assert!(filter.is_relevant(&path), "{} was ignored", path.display());
        }
        assert!(!filter.is_relevant(Path::new("/proj/elsewhere/stray.typ")));
    }

    #[test]
    fn the_banner_names_only_the_roots_that_are_there() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = Config::default();
        let root = Root::at(tmp.path());
        assert!(Filter::registered(&config, &root).is_empty());

        std::fs::create_dir_all(tmp.path().join(&config.paths.content)).expect("mkdir");
        assert_eq!(
            Filter::registered(&config, &root),
            vec![config.paths.content.as_path()]
        );
    }

    /// The whole scratch tree, not just `cache.dir`: the cache is one
    /// subdirectory of it.
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
        assert!(
            filter.is_relevant(&Path::new("/proj").join(&config.paths.content).join("a.typ")),
            "the edit that regenerates it has to rebuild on its own account"
        );
    }

    #[test]
    fn the_builds_own_output_never_triggers_a_rebuild() {
        let config = Config::default();
        let root = Root::at("/proj");
        let filter = Filter::new(&config, &root, Path::new("config.kdl")).unwrap();
        let dist = Path::new("/proj").join(&config.paths.dist);

        assert!(
            !filter.is_relevant(&dist.join(".assets.staging/style.css")),
            "a staged asset rebuilt the site that staged it"
        );
        assert!(!filter.is_relevant(&dist.join("index.html")));
        assert!(!filter.is_relevant(&dist.join("assets/main.js")));
        assert!(
            filter.is_relevant(
                &Path::new("/proj")
                    .join(&config.paths.assets)
                    .join("main.ts")
            ),
            "the edit that rebuilds the output has to be seen"
        );
    }

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
