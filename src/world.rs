use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use time::OffsetDateTime;
use typst::{
    Feature, Features, Library, LibraryExt, World,
    diag::FileResult,
    foundations::{Bytes, Datetime, Dict, IntoValue, Str, Value},
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

use crate::config::Config;
use crate::error::Result;
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
    fonts: Arc<FontStore>,
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
        } = self;
        (version, date, profile, git, site).hash(state);
    }
}

/// Git state of the site repository at build time.
#[derive(Debug, Clone, Hash)]
struct GitInfo {
    hash: String,
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
            version: env!("CARGO_PKG_VERSION"),
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
        }
    }

    /// The typst value placed at `sys.inputs.baudelaire`.
    fn to_value(&self) -> Value {
        let mut dict = Dict::new();
        dict.insert(Str::from("version"), self.version.into_value());
        dict.insert(Str::from("date"), self.date.clone().into_value());
        dict.insert(Str::from("mode"), self.mode.as_str().into_value());
        if let Some(profile) = &self.profile {
            dict.insert(Str::from("profile"), profile.clone().into_value());
        }
        if let Some(git) = &self.git {
            dict.insert(Str::from("git"), git.to_value());
        }
        dict.insert(Str::from("site"), self.site.to_value());
        dict.into_value()
    }
}

impl GitInfo {
    /// Read git state via the `git` CLI, or `None` outside a repository.
    fn detect(root: &Path) -> Option<Self> {
        let hash = git(root, &["rev-parse", "--short", "HEAD"])?;
        Some(Self {
            hash,
            branch: git(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
            tag: git(root, &["describe", "--tags", "--always"]),
            committed: git(root, &["log", "-1", "--format=%cI"]),
            dirty: git_dirty(root),
        })
    }

    fn to_value(&self) -> Value {
        let mut dict = Dict::new();
        dict.insert(Str::from("hash"), self.hash.clone().into_value());
        if let Some(branch) = &self.branch {
            dict.insert(Str::from("branch"), branch.clone().into_value());
        }
        if let Some(tag) = &self.tag {
            dict.insert(Str::from("tag"), tag.clone().into_value());
        }
        if let Some(committed) = &self.committed {
            dict.insert(Str::from("committed"), committed.clone().into_value());
        }
        dict.insert(Str::from("dirty"), self.dirty.into_value());
        dict.into_value()
    }
}

impl SiteInfo {
    fn to_value(&self) -> Value {
        let mut dict = Dict::new();
        if let Some(title) = &self.title {
            dict.insert(Str::from("title"), title.clone().into_value());
        }
        if let Some(url) = &self.url {
            dict.insert(Str::from("url"), url.clone().into_value());
        }
        dict.insert(Str::from("lang"), self.lang.clone().into_value());
        if let Some(author) = &self.author {
            dict.insert(Str::from("author"), author.clone().into_value());
        }
        dict.into_value()
    }
}

/// Run a `git` command in `root`, returning its trimmed stdout, or `None` if
/// git is absent, the command fails, or the output is empty.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).current_dir(root).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Whether the working tree has uncommitted changes.
fn git_dirty(root: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

impl Project {
    /// Build shared project state from a config, for the given build `mode`.
    pub fn new(config: &Config, mode: Mode) -> Result<Self> {
        let root = crate::fs::canonical(&config.content);
        let project_root = root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let mut fonts = FontStore::new();
        fonts.extend(typst_kit::fonts::system());

        let now = OffsetDateTime::now_utc();
        let context = BuildContext::detect(&project_root, now, config, mode);
        let mut inputs: Dict = config
            .inputs
            .iter()
            .map(|(k, v)| (Str::from(k.as_str()), v.clone().into_value()))
            .collect();
        // Reserved namespace exposing build metadata to pages.
        inputs.insert(Str::from("baudelaire"), context.to_value());

        let mut features = Vec::new();
        for name in &config.features {
            match FEATURES.iter().find(|(n, _)| n == name) {
                Some((_, feature)) => features.push(*feature),
                None => {
                    let valid = FEATURES.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ");
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
            fonts: Arc::new(fonts),
            files: Arc::new(FileStore::new(SystemFiles::new(
                FsRoot::new(project_root.clone()),
                SystemPackages::new(SystemDownloader::new(USER_AGENT)),
            ))),
            root: project_root,
            now,
            context,
        })
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

    /// Resolve a file id the compiler touched back to its filesystem path.
    pub fn path_of(&self, id: FileId) -> Option<PathBuf> {
        self.files.loader().resolve(id).ok()
    }

    /// The files a tracked compilation read, excluding its own `main` source
    /// (fingerprinted separately), resolved to canonical paths — a page's exact
    /// dependency set.
    pub fn dependencies<W: World>(&self, world: &Tracked<W>) -> Deps {
        let main = world.main();
        let files = world
            .accessed()
            .into_iter()
            .filter(|id| *id != main)
            .filter_map(|id| self.path_of(id))
            .filter_map(|p| crate::fs::canonicalize(p).ok())
            .collect();
        Deps::from_paths(files)
    }
}

/// A [`World`] wrapper that records every file the compiler reads, yielding a
/// compilation's exact dependency set — transitive imports, data loaders
/// (`json`, `csv`, …), and assets alike.
///
/// This works even though the underlying world is comemo-memoized and shared
/// across pages: comemo validates a cached result by re-calling the tracked
/// `source`/`file` accessors, so every dependency still flows through here.
/// Verified by `tests/incremental_e2e.rs` (`shared_module_tracked_for_every_page`,
/// `editing_transitive_import_invalidates_page`).
pub struct Tracked<W> {
    inner: W,
    accessed: std::sync::Mutex<std::collections::HashSet<FileId>>,
}

impl<W> Tracked<W> {
    /// Wrap a world to record its file accesses.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            accessed: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// The wrapped world.
    pub fn inner(&self) -> &W {
        &self.inner
    }

    /// The file ids accessed so far.
    pub fn accessed(&self) -> Vec<FileId> {
        self.accessed.lock().expect("lock").iter().copied().collect()
    }

    fn record(&self, id: FileId) {
        self.accessed.lock().expect("lock").insert(id);
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
        let offset =
            offset.and_then(|o| time::UtcOffset::from_whole_seconds(o.seconds() as i32).ok())?;
        let dt = self.project.now.checked_to_offset(offset)?;
        Some(Datetime::Date(dt.date()))
    }
}
