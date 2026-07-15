use std::path::{Path, PathBuf};
use std::process::Command;
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
use typst_kit::{
    downloader::SystemDownloader,
    files::{FileStore, FsRoot, SystemFiles},
    fonts::FontStore,
    packages::SystemPackages,
};

use crate::codegen;
use crate::config::Config;
use crate::error::{Result, TypstSourceDiagnostic};
use crate::graph::Deps;

const USER_AGENT: &str = concat!("baudelaire/", env!("CARGO_PKG_VERSION"));

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
    /// System fonts, discovered lazily on first glyph lookup — a fully-cached
    /// rebuild compiles nothing, so it never pays to scan the font directories.
    fonts: Arc<LazyLock<FontStore>>,
    files: Arc<FileStore<SystemFiles>>,
    root: PathBuf,
    now: OffsetDateTime,
    context: BuildContext,
}

/// How the site is being produced, exposed to pages as
/// `sys.inputs.baudelaire.mode` so they can branch (e.g. a dev banner in serve).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Build,
    Serve,
    Check,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Serve => "serve",
            Self::Check => "check",
        }
    }
}

/// Build metadata exposed to pages via `sys.inputs.baudelaire` (version, build
/// date, mode, active profile, git state, and a mirror of site identity). `Hash`
/// so it can salt the build cache.
#[derive(Debug, Clone)]
pub struct BuildContext {
    version: &'static str,
    date: String,
    mode: Mode,
    profile: Option<String>,
    git: Option<GitInfo>,
    site: SiteInfo,
    /// The `client { }` constants, also exposed to templates at
    /// `sys.inputs.baudelaire.client` (mirroring the `baudelaire:config` module).
    client: codegen::Value,
}

/// Feeds the cache fingerprint. `mode` is deliberately excluded: it is
/// informational only (exposed at `sys.inputs.baudelaire.mode`) and must not
/// key the cache, or `build` and `serve` — which differ only in mode — would
/// invalidate each other's cache on every switch. Destructuring keeps a new
/// field from being silently forgotten.
impl std::hash::Hash for BuildContext {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            version,
            date,
            mode: _,
            profile,
            git,
            site,
            // client constants already fold into the cache via `Config`'s own
            // hash, so hashing them here too would only double-count.
            client: _,
        } = self;
        (version, date, profile, git, site).hash(state);
    }
}

/// Git state of the site repository at build time.
#[derive(Debug, Clone, Hash)]
struct GitInfo {
    /// The full commit SHA — pages slice it themselves for a short form.
    hash: String,
    /// The revision number: how many commits are reachable from HEAD.
    rev: Option<String>,
    branch: Option<String>,
    tag: Option<String>,
    committed: Option<String>,
    dirty: bool,
}

/// A mirror of site identity, so layouts can read it via `sys.inputs` without
/// per-page frontmatter plumbing.
#[derive(Debug, Clone, Hash)]
struct SiteInfo {
    title: Option<String>,
    url: Option<String>,
    lang: String,
    author: Option<String>,
}

impl BuildContext {
    /// Detect build metadata for the site rooted at `root`.
    fn detect(root: &Path, now: OffsetDateTime, config: &Config, mode: Mode) -> Self {
        Self {
            version: crate::VERSION,
            date: now.date().to_string(),
            mode,
            profile: config.profile.clone(),
            git: GitInfo::detect(root),
            site: SiteInfo {
                title: config.site.clone(),
                url: config.url.clone(),
                lang: config.lang.clone(),
                author: config.author.clone(),
            },
            client: codegen::Value::dict(config.client.iter().cloned()),
        }
    }
}

/// The dictionary placed at `sys.inputs.baudelaire`, built once as a
/// [`codegen::Value`] and converted to a Typst runtime value at injection.
impl From<&BuildContext> for codegen::Value {
    fn from(cx: &BuildContext) -> Self {
        use codegen::Value;
        let mut fields = vec![
            ("version", Value::str(cx.version)),
            ("date", Value::str(&cx.date)),
            ("mode", Value::str(cx.mode.as_str())),
        ];
        if let Some(profile) = &cx.profile {
            fields.push(("profile", Value::str(profile)));
        }
        if let Some(git) = &cx.git {
            fields.push(("git", git.into()));
        }
        fields.push(("site", (&cx.site).into()));
        fields.push(("client", cx.client.clone()));
        Value::dict(fields)
    }
}

impl GitInfo {
    /// Read git state via the `git` CLI, or `None` outside a repository.
    fn detect(root: &Path) -> Option<Self> {
        let hash = Self::run(root, &["rev-parse", "HEAD"])?;
        Some(Self {
            hash,
            rev: Self::run(root, &["rev-list", "--count", "HEAD"]),
            branch: Self::run(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
            // No `--always`: it falls back to a bare commit hash in a tagless
            // repo, which would populate `git.tag` with a non-tag. An empty
            // output (no tag reachable) becomes `None` instead.
            tag: Self::run(root, &["describe", "--tags"]),
            committed: Self::run(root, &["log", "-1", "--format=%cI"]),
            // A non-empty `status --porcelain` means uncommitted changes.
            dirty: Self::run(root, &["status", "--porcelain"]).is_some(),
        })
    }

    /// Run a `git` command in `root`, returning its trimmed stdout, or `None`
    /// if git is absent, the command fails, or the output is empty. The single
    /// place this crate shells out to git.
    fn run(root: &Path, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!text.is_empty()).then_some(text)
    }
}

impl From<&GitInfo> for codegen::Value {
    fn from(git: &GitInfo) -> Self {
        use codegen::Value;
        let mut fields = vec![("hash", Value::str(&git.hash))];
        if let Some(rev) = &git.rev {
            fields.push(("rev", Value::str(rev)));
        }
        if let Some(branch) = &git.branch {
            fields.push(("branch", Value::str(branch)));
        }
        if let Some(tag) = &git.tag {
            fields.push(("tag", Value::str(tag)));
        }
        if let Some(committed) = &git.committed {
            fields.push(("committed", Value::str(committed)));
        }
        fields.push(("dirty", Value::Bool(git.dirty)));
        Value::dict(fields)
    }
}

impl From<&SiteInfo> for codegen::Value {
    fn from(site: &SiteInfo) -> Self {
        use codegen::Value;
        let mut fields: Vec<(&str, Value)> = Vec::new();
        if let Some(title) = &site.title {
            fields.push(("title", Value::str(title)));
        }
        if let Some(url) = &site.url {
            fields.push(("url", Value::str(url)));
        }
        fields.push(("lang", Value::str(&site.lang)));
        if let Some(author) = &site.author {
            fields.push(("author", Value::str(author)));
        }
        Value::dict(fields)
    }
}

impl Project {
    /// Build shared project state from a config, for the given build `mode`.
    pub fn new(config: &Config, mode: Mode) -> Result<Self> {
        let root = crate::fs::canonical(&config.content);
        let project_root = root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let now = OffsetDateTime::now_utc();
        let context = BuildContext::detect(&project_root, now, config, mode);
        let mut inputs: Dict = config
            .inputs
            .iter()
            .map(|(k, v)| (Str::from(k.as_str()), v.clone().into_value()))
            .collect();
        // reserved namespace exposing build metadata to pages.
        inputs.insert(
            Str::from("baudelaire"),
            Value::from(&codegen::Value::from(&context)),
        );

        let mut features = Vec::new();
        for name in &config.features {
            match FEATURES.iter().find(|(n, _)| n == name) {
                Some((_, feature)) => features.push(*feature),
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

        Ok(Self {
            lib: Arc::new(LazyHash::new(
                Library::builder()
                    .with_features(Features::from_iter(features))
                    .with_inputs(inputs)
                    .build(),
            )),
            fonts: Arc::new(LazyLock::new(Self::system_fonts)),
            files: Arc::new(FileStore::new(SystemFiles::new(
                FsRoot::new(project_root.clone()),
                SystemPackages::new(SystemDownloader::new(USER_AGENT)),
            ))),
            root: project_root,
            now,
            context,
        })
    }

    /// Discover and load the system fonts. Used as the [`LazyLock`] initializer
    /// for [`Project::fonts`] so the cost (scanning font directories, parsing
    /// fontconfig) is paid only when a page is actually compiled — never on a
    /// fully-cached rebuild.
    fn system_fonts() -> FontStore {
        let mut fonts = FontStore::new();
        // Typst's embedded defaults (Libertinus, New Computer Modern, DejaVu)
        // first, then system fonts — so a glyph resolves the same way it does
        // under `typst` itself, instead of falling back to whatever the system
        // happens to offer (which can rasterize digits as colour-font images).
        fonts.extend(typst_kit::fonts::embedded());
        fonts.extend(typst_kit::fonts::system());
        fonts
    }

    /// Build metadata injected into `sys.inputs.baudelaire`; folded into the
    /// cache fingerprint so a new commit or day rebuilds pages that embed it.
    pub fn context(&self) -> &BuildContext {
        &self.context
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
    /// store — discovery and compilation read one parse.
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

    /// Evaluate a source as a typst module — the compiler's own memoized
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
            let name = source.id().vpath().get_without_slash().to_string();
            crate::error::BaudelaireErrorKind::TypstCompile(TypstSourceDiagnostic::bridge(
                errs,
                (&name, source.text()),
                Arc::new(world),
            ))
        })
    }

    /// Evaluate a source as a typst module and capture the files the evaluation
    /// read — the frontmatter's exact dependency set, so discovery can cache the
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
                let name = source.id().vpath().get_without_slash().to_string();
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
    /// (fingerprinted separately), resolved to canonical paths — a page's exact
    /// dependency set.
    pub fn dependencies<W: World>(&self, world: &Tracked<W>) -> Deps {
        let main = world.main();
        world
            .accessed()
            .into_iter()
            .filter(|id| *id != main)
            .filter_map(|id| self.path_of(id))
            .filter_map(|p| crate::fs::canonicalize(p).ok())
            .collect::<Vec<_>>()
            .into()
    }
}

/// A [`World`] wrapper that records every file the compiler reads, yielding a
/// compilation's exact dependency set — transitive imports, data loaders
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
}

impl<W> Tracked<W> {
    /// Wrap a world to record its file accesses.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            accessed: parking_lot::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// The wrapped world.
    pub fn inner(&self) -> &W {
        &self.inner
    }

    /// Consume the wrapper, returning the wrapped world — for building an owned
    /// world (e.g. an `Arc`) once tracking is done.
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// The file ids accessed so far.
    pub fn accessed(&self) -> Vec<FileId> {
        self.accessed.lock().iter().copied().collect()
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
        // No offset defaults to UTC — the same clock the build context stamps —
        // rather than `None`, which typst reports as "unable to determine
        // current date" on every offset-less `datetime.today()` call.
        let offset = match offset {
            Some(o) => time::UtcOffset::from_whole_seconds(o.seconds() as i32).ok()?,
            None => time::UtcOffset::UTC,
        };
        let dt = self.project.now.checked_to_offset(offset)?;
        Some(Datetime::Date(dt.date()))
    }
}
