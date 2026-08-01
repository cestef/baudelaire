use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use time::OffsetDateTime;
use typst::{
    Feature, Features, Library, LibraryExt, World,
    comemo::Track,
    diag::{FileError, FileResult},
    engine::{Route, Sink, Traced},
    foundations::{Bytes, Datetime, Dict, IntoValue, Module, Str, Value},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
mod context;
pub mod image_rule;
pub(crate) mod module;
mod packages;

pub use context::{BuildContext, Mode};
pub(crate) use packages::Registry;

use typst_kit::{files::FileStore, files::FsRoot, fonts::FontStore, packages::SystemPackages};

use module::{Files, ModuleCx};

use crate::codegen;
use crate::config::Config;
use crate::error::{Result, TypstSourceDiagnostic};
use crate::graph::Deps;

pub(crate) const USER_AGENT: &str = concat!("baudelaire/", env!("CARGO_PKG_VERSION"));

/// The typst features exposable via `features` in config, as `(name, feature)`
/// pairs. Single source of truth: parsing a feature name and listing the valid
/// names in errors both read this table.
const FEATURES: &[(&str, Feature)] = &[
    ("html", Feature::Html),
    ("bundle", Feature::Bundle),
    ("a11y-extras", Feature::A11yExtras),
];

/// Shared project state: fonts, file loader, library. Cloned cheaply per
/// page compile so comemo memoization survives across the pool.
#[derive(Clone)]
pub struct Project {
    lib: Arc<LazyHash<Library>>,
    /// System fonts, discovered lazily on first glyph lookup: a fully-cached
    /// rebuild compiles nothing, so it never pays to scan the font directories.
    fonts: Arc<LazyLock<FontStore>>,
    files: Arc<FileStore<Files>>,
    root: PathBuf,
    now: OffsetDateTime,
    context: BuildContext,
}

impl Project {
    /// Build shared project state from a config, for the given build `mode`.
    pub fn new(config: &Config, mode: Mode) -> Result<Self> {
        let project_root = crate::fs::canonical(&config.root);

        let now = OffsetDateTime::now_utc();
        let context = BuildContext::detect(&project_root, now, config, mode);
        // One build-context tree, read twice: injected at `sys.inputs.baudelaire`
        // and served to the `@baudelaire/*` module registry.
        let tree = codegen::Value::from(&context);
        let mut inputs: Dict = config
            .typst
            .inputs
            .iter()
            .map(|(k, v)| (Str::from(k.as_str()), v.clone().into_value()))
            .collect();
        // reserved namespace exposing build metadata to pages.
        inputs.insert(Str::from("baudelaire"), Value::from(&tree));

        // HTML export is non-negotiable: Baudelaire only ever emits an
        // `HtmlDocument`, so `Feature::Html` is always on and can never be
        // disabled (`-html` is rejected at parse). Every other feature is a
        // `+name`/`-name` toggle resolved here in order, so a later entry wins.
        let mut features = vec![Feature::Html];
        for token in &config.typst.features {
            let (enable, name) = match token.strip_prefix('-') {
                Some(rest) => (false, rest),
                None => (true, token.as_str()),
            };
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
        // Externalize typst-embedded images (base64 -> file reference) by
        // overriding typst-html's native image rule. Off by default and skipped
        // when `html.embed` inlines everything anyway.
        if config.assets.images.externalize(&config.html) {
            library
                .rules
                .replace(typst::foundations::Target::Html, image_rule::IMAGE_RULE);
        }

        Ok(Self {
            lib: Arc::new(LazyHash::new(library)),
            fonts: Arc::new(LazyLock::new(Self::system_fonts)),
            files: Arc::new(FileStore::new(Files::new(
                &ModuleCx { context: &tree },
                &project_root,
                FsRoot::new(project_root.clone()),
                SystemPackages::from(Registry(config.typst.registry.as_deref())),
            ))),
            root: project_root,
            now,
            context,
        })
    }

    /// Discover and load the system fonts. Used as the [`LazyLock`] initializer
    /// for [`Project::fonts`] so the cost (scanning font directories, parsing
    /// fontconfig) is paid only when a page is actually compiled: never on a
    /// fully-cached rebuild.
    fn system_fonts() -> FontStore {
        let mut fonts = FontStore::new();
        // Typst's embedded defaults (Libertinus, New Computer Modern, DejaVu)
        // first, then system fonts, so a glyph resolves the same way it does
        // under `typst` itself, instead of falling back to whatever the system
        // happens to offer (which can rasterize digits as colour-font images).
        // Without the `embedded-fonts` feature the defaults are not bundled, so
        // resolution depends entirely on what the host provides.
        #[cfg(feature = "embedded-fonts")]
        fonts.extend(typst_kit::fonts::embedded());
        fonts.extend(typst_kit::fonts::system());
        fonts
    }

    /// Build metadata injected into `sys.inputs.baudelaire`.
    pub fn context(&self) -> &BuildContext {
        &self.context
    }

    /// The injected values whose per-page reads the cache tracks, each as a
    /// dotted base and its current tree. One entry today (build metadata), but
    /// the mechanism is generic, so any future `sys.inputs.*` value tracked for
    /// fine-grained invalidation is added here.
    pub fn tracked(&self) -> Vec<(String, codegen::Value)> {
        vec![(
            Self::METADATA.to_owned(),
            codegen::Value::from(&self.context),
        )]
    }

    /// A content fingerprint over the generated `@baudelaire/*` modules, for
    /// the build cache. A virtual module has no path, so it can never appear in
    /// a page's dependency set; see [`module`] for why this is sound.
    pub fn modules(&self) -> crate::graph::Hash {
        self.files.loader().fingerprint()
    }

    /// The dotted base naming build metadata in typst source.
    const METADATA: &'static str = "sys.inputs.baudelaire";

    /// The tracked key standing for "this build's clock".
    ///
    /// `datetime.today()` reads the same instant as
    /// `sys.inputs.baudelaire.date` but through the [`World`], where nothing
    /// records it: [`Tracked`] captures `source` and `file` only. A page
    /// printing the current year was therefore a cache hit into the next one.
    /// Recording the call as a read of this key reuses the value-digest
    /// invalidation already in place instead of inventing a second mechanism.
    pub fn clock() -> String {
        format!("{}.{}", Self::METADATA, BuildContext::DATE)
    }

    /// Project root directory.
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
        self.files.source(id).map_err(|e| {
            let kind = match &e {
                FileError::NotFound(_) => std::io::ErrorKind::NotFound,
                FileError::AccessDenied => std::io::ErrorKind::PermissionDenied,
                _ => std::io::ErrorKind::Other,
            };
            crate::error::FsError::new(crate::error::Op::Read, path, std::io::Error::new(kind, e))
                .into()
        })
    }

    /// Evaluate a source as a typst module: the compiler's own memoized
    /// evaluation, so a later compile of the same file reuses it. A module's
    /// scope carries the page's exports (`#let frontmatter = ..`); its errors
    /// carry real file spans.
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
            ))
        })
    }

    /// Evaluate a source as a typst module and capture the files the evaluation
    /// read: the frontmatter's exact dependency set, so discovery can cache the
    /// extracted frontmatter and re-evaluate only when a dependency changes.
    /// Like [`Project::module`] but through a [`Tracked`] world; the page's own
    /// source is excluded from the deps (it is fingerprinted separately).
    pub fn module_tracked(&self, source: &Source) -> Result<(Module, Deps)> {
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
                Ok((module, deps))
            }
            Err(errs) => {
                let name = source.id().vpath().get_without_slash().to_owned();
                Err(crate::error::BaudelaireErrorKind::TypstCompile(
                    TypstSourceDiagnostic::bridge(
                        errs,
                        (&name, source.text()),
                        Arc::new(world.into_inner()),
                    ),
                ))
            }
        }
    }

    /// Resolve a file id the compiler touched back to its filesystem path.
    pub fn path_of(&self, id: FileId) -> Option<PathBuf> {
        self.files.loader().resolve(id).ok()
    }

    /// The files a tracked compilation read, excluding its own `main` source
    /// (fingerprinted separately): a page's exact dependency set, canonicalized
    /// where the path resolves and kept as read where it does not.
    pub fn dependencies<W: World>(&self, world: &Tracked<W>) -> Deps {
        let main = world.main();
        world
            .accessed()
            .into_iter()
            .filter(|id| *id != main)
            .filter_map(|id| self.path_of(id))
            // A path that will not canonicalize (deleted between the read and
            // here, an editor's write-to-temp-then-rename under `serve`) must
            // not be dropped: a dependency that goes unrecorded is one the page
            // can never be invalidated by, and it would serve stale output
            // forever. That is the unsound direction, the same one
            // `graph::access` refuses when it cannot load a file. Keep the
            // lexical path instead (`fs::canonical` falls back to it), where
            // the cost is at worst one rebuild too many and the file's absence
            // is itself recorded as a dependency. Not a hard error either: that
            // rename race is routine, and failing on it would make `serve`
            // flaky over a file that is fine a millisecond later.
            .map(crate::fs::canonical)
            .collect::<Vec<_>>()
            .into()
    }
}

/// A [`World`] wrapper that records every file the compiler reads, yielding a
/// compilation's exact dependency set: transitive imports, data loaders
/// (`json`, `csv`, ..), and assets alike.
///
/// This works even though the underlying world is comemo-memoized and shared
/// across pages: comemo validates a cached result by re-calling the tracked
/// `source`/`file` accessors, so every dependency still flows through here.
/// Verified by `tests/incremental_e2e.rs` (`shared_module_tracked_for_every_page`,
/// `editing_transitive_import_invalidates_page`).
pub struct Tracked<W> {
    inner: W,
    accessed: parking_lot::Mutex<std::collections::HashSet<FileId>>,
    /// Whether the compilation asked for the current date. Not a file access,
    /// so it needs its own flag; see [`Project::clock`] for why it is recorded.
    clock: std::sync::atomic::AtomicBool,
}

impl<W> Tracked<W> {
    /// Wrap a world to record its file accesses.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            accessed: parking_lot::Mutex::new(std::collections::HashSet::new()),
            clock: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The wrapped world.
    pub fn inner(&self) -> &W {
        &self.inner
    }

    /// Consume the wrapper, returning the wrapped world, for building an owned
    /// world (e.g. an `Arc`) once tracking is done.
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// The file ids accessed so far.
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

/// A world bound to a single page's source as `main`. Shares project fonts,
/// files, and library so comemo caches hit across compiles.
#[derive(Clone)]
pub struct PageWorld {
    project: Project,
    main: Source,
}

impl PageWorld {
    /// The main source file id.
    pub fn id(&self) -> FileId {
        self.main.id()
    }

    /// The main source.
    pub fn source(&self) -> &Source {
        &self.main
    }

    /// This build's date, for an exporter that stamps one into its output.
    ///
    /// Deliberately not [`World::today`]: that call is what [`Tracked`] records
    /// as a read of the clock, and a page that merely *ships* a dated PDF has
    /// not displayed the date and must not be invalidated when it turns over.
    /// It is the same instant either way, so the PDF's creation date and
    /// `sys.inputs.baudelaire.date` cannot disagree.
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
        self.project.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.project.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.project.fonts.font(index)
    }

    fn today(&self, offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        // No offset defaults to UTC (the same clock the build context stamps)
        // rather than `None`, which typst reports as "unable to determine
        // current date" on every offset-less `datetime.today()` call.
        let offset = match offset {
            // Clamped before narrowing, so the cast cannot truncate: an offset
            // that overflows `i32` is thousands of times past the day
            // `from_whole_seconds` accepts, and is `None` either way.
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
