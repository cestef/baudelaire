use std::path::{Path, PathBuf};
use std::sync::Arc;

use time::OffsetDateTime;
use typst::{
    Feature, Features, Library, LibraryExt, World,
    comemo::Track,
    diag::FileResult,
    engine::{Route, Sink, Traced},
    foundations::{Bytes, Datetime, Dict, IntoValue, Module, Str, Value},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
mod context;
mod fonts;
pub(crate) mod generated;
#[cfg(feature = "markdown")]
mod markdown;
pub(crate) mod module;
mod packages;
pub mod rules;

pub use context::{BuildContext, Mode};
pub(crate) use packages::Registry;

use parking_lot::RwLock;
use typst_kit::{files::FileStore, files::FsRoot, packages::SystemPackages};

use fonts::Fonts;
use module::{Files, ModuleCx};

use crate::codegen;
use crate::config::Config;
use crate::error::{Result, TypstSourceDiagnostic};
use crate::graph::Deps;

pub(crate) const USER_AGENT: &str = concat!("baudelaire/", env!("CARGO_PKG_VERSION"));

/// The typst features exposable via `features` in config, as `(name, feature)`
/// pairs; `html` is force-enabled on top and `-html` is refused at parse.
const FEATURES: &[(&str, Feature)] = &[
    ("html", Feature::Html),
    ("bundle", Feature::Bundle),
    ("a11y-extras", Feature::A11yExtras),
];

/// Shared project state: fonts, file loader, library, cloned cheaply per page
/// compile so comemo memoization survives across the pool.
#[derive(Clone)]
pub struct Project {
    lib: Arc<LazyHash<Library>>,
    fonts: Arc<Fonts>,
    /// Behind a lock because one build writes files the store has already
    /// served: see [`Project::tables_written`].
    files: Arc<RwLock<FileStore<Files>>>,
    root: PathBuf,
    now: OffsetDateTime,
    context: BuildContext,
}

impl Project {
    /// Build shared project state from a config, for the given build `mode`.
    ///
    /// `theme` is already resolved, and must be the one the rest of the build
    /// layers assets from.
    pub fn new(config: &Config, mode: Mode, theme: Option<&crate::theme::Theme>) -> Result<Self> {
        let project_root = crate::fs::canonical(&config.root);

        let now = OffsetDateTime::now_utc();
        let context = BuildContext::detect(&project_root, now, config, mode);
        let tree = codegen::Value::from(&context);
        let mut inputs: Dict = config
            .typst
            .inputs
            .iter()
            .map(|(k, v)| (Str::from(k.as_str()), v.clone().into_value()))
            .collect();
        inputs.insert(Str::from("baudelaire"), Value::from(&tree));

        let mut features = vec![Feature::Html];
        for token in &config.typst.features {
            let (enable, name) = token
                .strip_prefix('-')
                .map_or((true, token.as_str()), |rest| (false, rest));
            match FEATURES.iter().find(|(n, _)| *n == name) {
                Some((_, feature)) if enable => {
                    if !features.contains(feature) {
                        features.push(*feature);
                    }
                }
                Some((_, feature)) => features.retain(|f| f != feature),
                None => {
                    let valid = FEATURES
                        .iter()
                        .map(|(n, _)| *n)
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(crate::error::ConfigError::unknown_feature(name, &valid).into());
                }
            }
        }

        let mut library = Library::builder()
            .with_features(Features::from_iter(features))
            .with_inputs(inputs)
            .build();
        #[cfg(feature = "markdown")]
        markdown::define(&mut library);
        rules::Rules::install(&mut library, config);

        Ok(Self {
            lib: Arc::new(LazyHash::new(library)),
            fonts: Arc::new(Fonts::of(&config.typst.fonts, &project_root)),
            files: Arc::new(RwLock::new(FileStore::new(Files::new(
                &ModuleCx {
                    context: &tree,
                    #[cfg(feature = "markdown")]
                    markdown: &config.content.markdown,
                    sources: &config.paths.sources,
                },
                &project_root,
                FsRoot::new(project_root.clone()),
                SystemPackages::from(Registry(config.typst.registry.as_deref())),
                theme
                    .and_then(crate::theme::Theme::mount)
                    .map(|(prefix, root)| (prefix, root.to_path_buf())),
                &config.paths.sources,
            )))),
            root: project_root,
            now,
            context,
        })
    }

    /// Build metadata injected into `sys.inputs.baudelaire`.
    pub fn context(&self) -> &BuildContext {
        &self.context
    }

    /// The injected values whose per-page reads the cache tracks, each as a
    /// dotted base and its current tree.
    pub fn tracked(&self) -> Vec<(String, codegen::Value)> {
        vec![(
            Self::METADATA.to_owned(),
            codegen::Value::from(&self.context),
        )]
    }

    /// A content fingerprint over the generated `@baudelaire/*` modules, which
    /// have no path and so can never appear in a page's dependency set.
    pub fn modules(&self) -> crate::graph::Hash {
        self.files.read().loader().fingerprint()
    }

    /// A content fingerprint over the faces the site ships itself, or `None`
    /// when it ships none; see [`fonts::Fonts::digest`] for why it is needed.
    pub fn fonts(&self) -> Option<crate::graph::Hash> {
        self.fonts.digest()
    }

    /// The generated tables (`@baudelaire/sections`, `@baudelaire/pages`) are
    /// on disk; called once per build, by the pass that writes them.
    ///
    /// A page evaluated during discovery was served the empty table, so the
    /// loaded slots are dropped here for its compile to read the real one.
    pub fn tables_written(&self) {
        let mut files = self.files.write();
        if files.loader().published() {
            files.reset();
        }
    }

    /// The dotted base naming build metadata in typst source.
    const METADATA: &'static str = "sys.inputs.baudelaire";

    /// The tracked key standing for "this build's clock".
    ///
    /// `datetime.today()` reads the same instant through the [`World`], which
    /// records no file, so the call is recorded as a read of this key instead.
    pub fn clock() -> String {
        format!("{}.{}", Self::METADATA, BuildContext::DATE)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a world for compiling a single source file as `main`.
    pub fn world_for(&self, source: &Source) -> PageWorld {
        PageWorld {
            project: self.clone(),
            main: source.clone(),
        }
    }

    /// Virtualize a filesystem path under the project root.
    pub fn virtualize(&self, path: &Path) -> Result<RootedPath> {
        let canon = crate::fs::canonical(path);
        let vpath = VirtualPath::virtualize(self.root(), &canon)?;
        Ok(RootedPath::new(VirtualRoot::Project, vpath))
    }

    /// The parsed source of a project file, loaded through the shared file
    /// store: discovery and compilation read one parse.
    pub fn source(&self, path: &Path) -> Result<Source> {
        let id = FileId::new(self.virtualize(path)?);
        self.files
            .read()
            .source(id)
            .map_err(|e| crate::error::typ::TypstFileError::of(path, &e).into())
    }

    /// Evaluate a source as a typst module, through the compiler's own
    /// memoized evaluation, so a later compile of the same file reuses it.
    pub fn module(&self, source: &Source) -> Result<Module> {
        let world = self.world_for(source);
        let mut sink = Sink::new();
        let traced = Traced::default();
        typst_eval::eval(
            (&world as &dyn World).track(),
            &self.lib,
            traced.track(),
            sink.track_mut(),
            Route::default().track(),
            source,
        )
        .map_err(|errs| {
            let name = source.id().vpath().get_without_slash().to_owned();
            crate::error::BaudelaireErrorKind::TypstCompile(TypstSourceDiagnostic::bridge(
                errs,
                (&name, source.text()),
                Arc::new(world),
                None,
            ))
        })
    }

    /// Like [`Project::module`] but through a [`Tracked`] world, also yielding
    /// the files the evaluation read (excluding the page's own source) and
    /// whether it read the build clock.
    pub fn module_tracked(&self, source: &Source) -> Result<(Module, Deps, bool)> {
        let world = Tracked::new(self.world_for(source));
        let mut sink = Sink::new();
        let traced = Traced::default();
        let result = typst_eval::eval(
            (&world as &dyn World).track(),
            &self.lib,
            traced.track(),
            sink.track_mut(),
            Route::default().track(),
            source,
        );
        match result {
            Ok(module) => {
                let deps = self.dependencies(&world);
                Ok((module, deps, world.reads_clock()))
            }
            Err(errs) => {
                let name = source.id().vpath().get_without_slash().to_owned();
                Err(crate::error::BaudelaireErrorKind::TypstCompile(
                    TypstSourceDiagnostic::bridge(
                        errs,
                        (&name, source.text()),
                        Arc::new(world.into_inner()),
                        None,
                    ),
                ))
            }
        }
    }

    /// Resolve a file id the compiler touched back to its filesystem path.
    pub fn path_of(&self, id: FileId) -> Option<PathBuf> {
        self.files.read().loader().resolve(id).ok()
    }

    /// The files a tracked compilation read, excluding its own `main` source,
    /// canonicalized where the path resolves and kept lexically where it does
    /// not, since a dependency that goes unrecorded can never invalidate.
    pub fn dependencies<W: World>(&self, world: &Tracked<W>) -> Deps {
        let main = world.main();
        world
            .accessed()
            .into_iter()
            .filter(|id| *id != main)
            .filter_map(|id| self.path_of(id))
            .map(crate::fs::canonical)
            .collect::<Vec<_>>()
            .into()
    }
}

/// A [`World`] wrapper that records every file the compiler reads: transitive
/// imports, data loaders (`json`, `csv`, ..), and assets alike.
///
/// Memoization does not hide a read: comemo validates a cached result by
/// re-calling the tracked `source`/`file` accessors.
pub struct Tracked<W> {
    inner: W,
    accessed: parking_lot::Mutex<std::collections::HashSet<FileId>>,
    /// Whether the compilation asked for the current date, which is not a file
    /// access and so needs its own flag.
    clock: std::sync::atomic::AtomicBool,
}

impl<W> Tracked<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            accessed: parking_lot::Mutex::new(std::collections::HashSet::new()),
            clock: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn inner(&self) -> &W {
        &self.inner
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn accessed(&self) -> Vec<FileId> {
        self.accessed.lock().iter().copied().collect()
    }

    /// Whether the compilation read the build clock (`datetime.today()`).
    pub fn reads_clock(&self) -> bool {
        self.clock.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn record(&self, id: FileId) {
        self.accessed.lock().insert(id);
    }
}

impl<W: World> World for Tracked<W> {
    fn library(&self) -> &LazyHash<Library> {
        self.inner.library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.inner.book()
    }

    fn main(&self) -> FileId {
        self.inner.main()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.record(id);
        self.inner.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.record(id);
        self.inner.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.inner.font(index)
    }

    fn today(&self, offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        self.clock.store(true, std::sync::atomic::Ordering::Relaxed);
        self.inner.today(offset)
    }
}

/// How a page's synthetic layout module is named to the compiler.
///
/// A sibling of the page, so a relative template import resolves the same way,
/// but a distinct file, so it can `#include` the page without shadowing it as
/// `main`.
pub struct Wrapper;

impl Wrapper {
    /// What distinguishes a wrapper's name from the page it wraps, spelled so
    /// no real file can carry it.
    const SUFFIX: &'static str = "@layout";

    /// The wrapper module's file id for the page at `rooted`.
    pub fn id(rooted: &RootedPath) -> FileId {
        let name = format!("{}{}", rooted.vpath().get_without_slash(), Self::SUFFIX);
        let vpath = VirtualPath::new(&name)
            .expect("a page vpath with a suffix stays a valid relative vpath");
        FileId::new(RootedPath::new(VirtualRoot::Project, vpath))
    }

    /// The page behind a wrapper's name, or `path` unchanged when it names no
    /// wrapper.
    pub fn page(path: &str) -> &str {
        path.strip_suffix(Self::SUFFIX).unwrap_or(path)
    }
}

/// A world bound to a single page's source as `main`, sharing project fonts,
/// files and library so comemo caches hit across compiles.
#[derive(Clone)]
pub struct PageWorld {
    project: Project,
    main: Source,
}

impl PageWorld {
    pub fn id(&self) -> FileId {
        self.main.id()
    }

    pub fn source(&self) -> &Source {
        &self.main
    }

    /// This build's date, for an exporter that stamps one into its output.
    ///
    /// Deliberately not [`World::today`], which [`Tracked`] records as a read
    /// of the clock: shipping a dated PDF is not displaying the date.
    pub fn stamp(&self) -> Option<Datetime> {
        Some(Datetime::Date(self.project.now.date()))
    }
}

impl World for PageWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.project.lib
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.project.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        self.project.files.read().source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.project.files.read().file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.project.fonts.font(index)
    }

    /// The offset is clamped before the narrowing, so the cast cannot truncate.
    fn today(&self, offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        let offset = match offset {
            #[allow(clippy::cast_possible_truncation)]
            Some(o) => time::UtcOffset::from_whole_seconds(
                o.seconds().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
            )
            .ok()?,
            None => time::UtcOffset::UTC,
        };
        let dt = self.project.now.checked_to_offset(offset)?;
        Some(Datetime::Date(dt.date()))
    }
}
