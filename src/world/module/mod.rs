//! Virtual Typst modules: the `@baudelaire/*` packages a template can import
//! without anything existing on disk. Each [`Module`] generates Typst source
//! from the site's build data; [`Files`] is the single [`FileLoader`] that
//! serves them all, assembled from the registry in [`builtin`] and falling
//! through to the filesystem for everything else. Adding a module is one impl
//! and one line.
//!
//! This is the Typst-side mirror of [`crate::engine::asset::module`], which
//! serves `baudelaire:*` to the JavaScript bundler. The two read alike on
//! purpose: same trait shape, same registry function, same context struct.
//!
//! Typst has no notion of a package that isn't a directory, so each module is
//! served as a two-file package: a generated [`MANIFEST`] naming [`ENTRYPOINT`],
//! and the entrypoint itself. The namespace is arbitrary in a package
//! specifier (`@preview`, `@local`), so `@baudelaire` needs no cooperation from
//! the registry and never touches the network.
//!
//! **Nothing volatile may be baked into a module's source.** Build metadata
//! that changes between builds (the git state, the date) stays on
//! `sys.inputs.baudelaire`, where [`crate::graph::access`] tracks it per page
//! and rebuilds only the pages that display the value that moved. A module's
//! source is fingerprinted whole (see [`Files::fingerprint`]), so baking a
//! commit hash in would rebuild the entire site on every commit.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use typst::diag::{FileError, FileResult, PackageError};
use typst::ecow::EcoString;
use typst::foundations::Bytes;
use typst::syntax::{
    FileId, VirtualPath, VirtualRoot,
    package::{PackageSpec, PackageVersion},
};
use typst_kit::files::{FileLoader, FsRoot, SystemFiles};
use typst_kit::packages::SystemPackages;

pub mod package;

mod builtin;

pub use builtin::Html;
pub use package::{Package, Packages};

use super::generated::Table;
use crate::codegen::{Let, Value};
use crate::config::dispatch::Keys;
use crate::graph::Hash;

/// The package namespace every generated module lives in.
const NAMESPACE: &str = "baudelaire";

/// The version every generated module is served at.
///
/// Typst's package grammar requires a version (`parse_version` rejects an empty
/// one), so there is no unversioned form to fall back to and the only question
/// is what the number means. It versions the *module API*, never the binary:
/// pinning it to baudelaire's own version would break every template on every
/// release, which is the opposite of what a pin is for. A template that writes
/// `@baudelaire/html:0.1.0` keeps working across releases, and only a breaking
/// change to what these modules export bumps it.
///
/// One version is served today. Supporting an old one alongside a new one is
/// additive when the need arrives: [`Module::source`] gains the requested
/// version, and [`Modules::load`] stops rejecting it.
const VERSION: PackageVersion = PackageVersion {
    major: 0,
    minor: 1,
    patch: 0,
};

/// The manifest typst reads to find a package's entrypoint, and the entrypoint
/// every generated manifest names. Both are typst's own conventions.
const MANIFEST: &str = "typst.toml";
const ENTRYPOINT: &str = "lib.typ";

/// The read-only build data every virtual module generates from.
pub(super) struct ModuleCx<'a> {
    /// The `sys.inputs.baudelaire` value, so `@baudelaire/site` and its
    /// JavaScript counterpart `baudelaire:site` serve one build context.
    pub context: &'a Value,
}

/// One provider of an `@baudelaire/*` Typst module: a set of generated
/// bindings over a hand-written body.
trait Module {
    /// The name this module is imported under: `@baudelaire/<name>`.
    fn name(&self) -> &'static str;

    /// The values bound at the top of the module, in emission order. Build data
    /// only: everything a module generates from reaches Typst through here.
    fn bindings(&self, cx: &ModuleCx) -> Vec<(String, Value)>;

    /// The module's hand-written Typst, appended after the bindings, so a
    /// binding is always in scope for the code that closes over it.
    fn body(&self) -> &'static str;

    /// The Typst source of the module's entrypoint. Every generated value goes
    /// through [`Let`], so no module formats Typst (or its escaping) by hand.
    fn source(&self, cx: &ModuleCx) -> String {
        let mut out = String::new();
        for (name, value) in self.bindings(cx) {
            let _ = writeln!(out, "{}", Let(&name, &value));
        }
        out.push_str(self.body());
        out
    }
}

/// The registered virtual modules.
fn builtin() -> [Box<dyn Module>; 2] {
    [Box::new(builtin::Html), Box::new(builtin::Site)]
}

/// The modules whose source the build writes to a file instead of generating
/// into memory, because each is derived from the whole page set: the site's
/// section tree and its page catalogue.
///
/// THE list of them: [`Modules::new`] registers exactly these as
/// [`Entrypoint::File`], and [`crate::engine::compile::Generated`] writes one
/// file per entry, so a module cannot be served without being written or the
/// other way round.
pub(crate) const FILES: [&str; 2] = [SECTIONS, PAGES];

/// The section tree module: `sections(lang)`.
pub(crate) const SECTIONS: &str = "sections";

/// The page catalogue module: `pages(lang)`.
pub(crate) const PAGES: &str = "pages";

/// Where a module's entrypoint source comes from.
enum Entrypoint {
    /// Generated with the world and served from memory. Holds nothing volatile,
    /// so the registry fingerprint covers it for the whole site at once.
    Memory(Bytes),
    /// A file the build writes before any page compiles, served by path.
    ///
    /// The distinction is the whole point of this variant: a module served from
    /// memory has no path, so no page can depend on it *individually*, and a
    /// module derived from the whole page set would then have to invalidate
    /// every page whenever any page moved. Served as a file, typst's own
    /// dependency tracking records the read per page and scopes it to the
    /// templates that import it.
    File(PathBuf),
}

/// The modules of one build, as `name -> entrypoint`.
struct Modules {
    entrypoints: BTreeMap<&'static str, Entrypoint>,
    /// Whether this build has written the file-backed tables yet, and whether
    /// anything read one before it did. See [`Modules::published`].
    tables: Tables,
}

/// How far this build has got with the file-backed tables.
///
/// A content page may import one, and discovery evaluates a page whole to reach
/// its `frontmatter`, so the read can land before the build has anything to
/// write: the catalogue is derived from the frontmatter being read. The table
/// answers empty then, and this records that it happened, because the store
/// caches what it was handed and the compile must not be given that answer.
#[derive(Default)]
struct Tables {
    written: AtomicBool,
    read_early: AtomicBool,
}

impl Modules {
    fn new(cx: &ModuleCx, root: &Path) -> Self {
        let mut entrypoints: BTreeMap<&'static str, Entrypoint> = builtin()
            .iter()
            .map(|module| {
                (
                    module.name(),
                    Entrypoint::Memory(Bytes::from_string(module.source(cx))),
                )
            })
            .collect();
        for name in FILES {
            entrypoints.insert(name, Entrypoint::File(root.join(Table::of(name))));
        }
        Self {
            entrypoints,
            tables: Tables::default(),
        }
    }

    /// Record that this build's tables are on disk, and report whether anything
    /// read one before they were: the caller's cue to drop what the file store
    /// cached, so the compile reads the tables this build wrote rather than the
    /// empty (or previous) one discovery was handed.
    fn published(&self) -> bool {
        self.tables.written.store(true, Ordering::Relaxed);
        self.tables.read_early.load(Ordering::Relaxed)
    }

    /// The file backing `id`, when it names a file-backed module's entrypoint.
    /// The one place that mapping is made, so serving and path resolution can
    /// never disagree about where a module's source is.
    fn file(&self, id: FileId) -> Option<&Path> {
        let spec = Self::owner(&id)?;
        if id.vpath().get_without_slash() != ENTRYPOINT {
            return None;
        }
        match self.entrypoints.get(spec.name.as_str())? {
            Entrypoint::File(path) => Some(path),
            Entrypoint::Memory(_) => None,
        }
    }

    /// The generated module `id` names a file of, or `None` for a real file.
    /// The single test for "is this ours": both serving and path resolution go
    /// through it, so the namespace is matched in one place.
    fn owner(id: &FileId) -> Option<&PackageSpec> {
        match id.root() {
            VirtualRoot::Package(spec) if spec.namespace == NAMESPACE => Some(spec),
            _ => None,
        }
    }

    /// Serve a file from the generated module named by `spec`.
    fn load(&self, spec: &PackageSpec, vpath: &VirtualPath) -> FileResult<Bytes> {
        let Some((name, entrypoint)) = self.entrypoints.get_key_value(spec.name.as_str()) else {
            // `NotFound` would send the reader looking for a package to
            // install; naming the ones that exist is the whole answer.
            return Err(FileError::Package(PackageError::Other(Some(
                self.unknown(&spec.name),
            ))));
        };
        if spec.version != VERSION {
            return Err(FileError::Package(PackageError::Other(Some(
                Self::wrong_version(&spec.name),
            ))));
        }
        match vpath.get_without_slash() {
            MANIFEST => Ok(Bytes::from_string(Self::manifest(&spec.name))),
            ENTRYPOINT => match entrypoint {
                Entrypoint::Memory(source) => Ok(source.clone()),
                // Written by the build before any page compiles, so a template
                // reads the real table. A content page importing one is read
                // earlier than that, during discovery: the table it is asking
                // for is derived from the frontmatter that read is producing,
                // so the honest answer there is the empty one every module
                // already promises for a language that was not built. The read
                // is recorded, and `published` hands the caller the job of
                // discarding it once the real table lands.
                Entrypoint::File(path) => {
                    if !self.tables.written.load(Ordering::Relaxed) {
                        self.tables.read_early.store(true, Ordering::Relaxed);
                    }
                    match std::fs::read(path) {
                        Ok(bytes) => Ok(Bytes::new(bytes)),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            Ok(Bytes::from_string(Table::empty(name).source()))
                        }
                        Err(e) => Err(FileError::from_io(e, path)),
                    }
                }
            },
            // A module is exactly two files, so any other path is a typo in a
            // deep import (`@baudelaire/html:0.1.0/extra.typ`).
            _ => Err(FileError::NotFound(PathBuf::from(vpath.get_with_slash()))),
        }
    }

    /// The message for an import of a module that does not exist: the same
    /// nearest-match suggestion an unknown config key gets, off the registry
    /// itself so it can never name a module that stopped existing.
    fn unknown(&self, name: &str) -> EcoString {
        let names: Vec<&str> = self.entrypoints.keys().copied().collect();
        EcoString::from(format!(
            "unknown baudelaire module `{name}`, {}",
            Keys::of(&names).help(name, "modules")
        ))
    }

    /// The message for an import at a version the registry does not serve.
    ///
    /// Replaces typst's own `VersionNotFound`, which reads "latest is X" and
    /// leaves the reader to rewrite the specifier themselves. There is one
    /// version and nothing to choose between, so the message is simply the line
    /// to write. It also says what the number *is*, because the natural guess is
    /// baudelaire's own version, which this deliberately does not track.
    fn wrong_version(name: &str) -> EcoString {
        EcoString::from(format!(
            "`@baudelaire/{name}` is served only at {VERSION}: write \
             `@baudelaire/{name}:{VERSION}`. That version tracks what these \
             modules export, not baudelaire's own version, so it does not move \
             when baudelaire is upgraded."
        ))
    }

    /// The manifest served for a module, naming it and its entrypoint. Typst
    /// validates the name and version against the specifier, so both must
    /// echo what was asked for.
    fn manifest(name: &str) -> String {
        format!(
            "[package]\nname = \"{name}\"\nversion = \"{VERSION}\"\nentrypoint = \"{ENTRYPOINT}\"\n"
        )
    }

    /// A content fingerprint over the in-memory modules, for the build cache.
    ///
    /// One of these has no path to hash, so an edited module would otherwise
    /// leave every importing page a cache hit on the old source. Fingerprinting
    /// them whole is sound and, because their sources hold nothing volatile,
    /// changes only when baudelaire or the site's config does.
    ///
    /// File-backed modules are excluded, and must be: their content changes
    /// with the page set, and folding that in here would invalidate every page
    /// on every structural edit, which is the cost serving them as files exists
    /// to avoid. Each is a real dependency of the pages that import it.
    fn fingerprint(&self) -> Hash {
        let memory: BTreeMap<&&str, &Bytes> = self
            .entrypoints
            .iter()
            .filter_map(|(name, entrypoint)| match entrypoint {
                Entrypoint::Memory(source) => Some((name, source)),
                Entrypoint::File(_) => None,
            })
            .collect();
        Hash::of(&memory)
    }
}

/// The project's file loader: generated `@baudelaire/*` modules first,
/// everything else from the filesystem through typst-kit's own loader.
pub(super) struct Files {
    modules: Modules,
    system: SystemFiles,
}

impl Files {
    pub(super) fn new(
        cx: &ModuleCx,
        root: &Path,
        project: FsRoot,
        packages: SystemPackages,
    ) -> Self {
        Self {
            modules: Modules::new(cx, root),
            system: SystemFiles::new(project, packages),
        }
    }

    /// The filesystem path of `id`.
    ///
    /// A generated module has none, and must short-circuit: the system loader
    /// would try to *download* `@baudelaire/html` from the package registry.
    pub(super) fn resolve(&self, id: FileId) -> FileResult<PathBuf> {
        // A file-backed module resolves to the file it is served from, which is
        // what puts it in the importing page's dependency set.
        if let Some(path) = self.modules.file(id) {
            return Ok(path.to_path_buf());
        }
        if Modules::owner(&id).is_some() {
            return Err(FileError::Other(Some(EcoString::from(
                "a generated module has no file system path",
            ))));
        }
        self.system.resolve(id)
    }

    /// A content fingerprint over the generated modules, for the build cache.
    pub(super) fn fingerprint(&self) -> Hash {
        self.modules.fingerprint()
    }

    /// The build has written its file-backed tables; `true` if one was served
    /// before that, and the store is holding an answer that is now wrong.
    pub(super) fn published(&self) -> bool {
        self.modules.published()
    }
}

impl FileLoader for Files {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match Modules::owner(&id) {
            Some(spec) => self.modules.load(spec, id.vpath()),
            None => self.system.load(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::package::PackageSpec;

    /// Nothing on disk may become the source of truth: the build answers this
    /// namespace from memory, and an install that could shadow it would make a
    /// stale mirror a wrong build.
    #[test]
    fn the_registry_serves_memory_regardless_of_what_is_installed() {
        let modules = Modules::new(
            &ModuleCx {
                context: &Value::None,
            },
            Path::new("."),
        );
        let spec = PackageSpec {
            namespace: NAMESPACE.into(),
            name: "html".into(),
            version: VERSION,
        };
        let served = modules
            .load(&spec, &VirtualPath::new(ENTRYPOINT).expect("a valid vpath"))
            .expect("html is served");
        assert!(String::from_utf8_lossy(&served).contains(Html.body()));
    }
}
