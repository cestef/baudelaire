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
use std::path::PathBuf;

use typst::diag::{FileError, FileResult, PackageError};
use typst::ecow::EcoString;
use typst::foundations::Bytes;
use typst::syntax::{
    FileId, VirtualPath, VirtualRoot,
    package::{PackageSpec, PackageVersion},
};
use typst_kit::files::{FileLoader, FsRoot, SystemFiles};
use typst_kit::packages::SystemPackages;

use crate::codegen::{Typst, Value};
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

/// One provider of an `@baudelaire/*` Typst module.
trait Module {
    /// The name this module is imported under: `@baudelaire/<name>`.
    fn name(&self) -> &'static str;

    /// The Typst source of the module's entrypoint.
    fn source(&self, cx: &ModuleCx) -> String;
}

/// The registered virtual modules.
fn builtin() -> [Box<dyn Module>; 2] {
    [Box::new(Html), Box::new(Site)]
}

/// The generated modules of one build, as `name -> entrypoint source`.
struct Modules {
    sources: BTreeMap<&'static str, Bytes>,
}

impl Modules {
    fn new(cx: &ModuleCx) -> Self {
        let sources = builtin()
            .iter()
            .map(|module| (module.name(), Bytes::from_string(module.source(cx))))
            .collect();
        Self { sources }
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
        let Some(source) = self.sources.get(spec.name.as_str()) else {
            // `NotFound` would send the reader looking for a package to
            // install; naming the ones that exist is the whole answer.
            return Err(FileError::Package(PackageError::Other(Some(
                self.unknown(&spec.name),
            ))));
        };
        if spec.version != VERSION {
            // Reads as "version X does not exist (latest is Y)", pointing at
            // the import's own span.
            return Err(FileError::Package(PackageError::VersionNotFound(
                spec.clone(),
                VERSION,
            )));
        }
        match vpath.get_without_slash() {
            MANIFEST => Ok(Bytes::from_string(Self::manifest(&spec.name))),
            ENTRYPOINT => Ok(source.clone()),
            // A module is exactly two files, so any other path is a typo in a
            // deep import (`@baudelaire/html:0.1.0/extra.typ`).
            _ => Err(FileError::NotFound(PathBuf::from(vpath.get_with_slash()))),
        }
    }

    /// The message for an import of a module that does not exist: the same
    /// nearest-match suggestion an unknown config key gets, off the registry
    /// itself so it can never name a module that stopped existing.
    fn unknown(&self, name: &str) -> EcoString {
        let names: Vec<&str> = self.sources.keys().copied().collect();
        EcoString::from(format!(
            "unknown baudelaire module `{name}`, {}",
            Keys::of(&names).help(name, "modules")
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

    /// A content fingerprint over every generated module, for the build cache.
    ///
    /// No page's dependency set can carry a virtual module (it has no path to
    /// hash), so an edited module would otherwise leave every importing page a
    /// cache hit on the old source. Fingerprinting the registry whole is sound
    /// and, because module sources hold nothing volatile, changes only when
    /// baudelaire or the site's config does.
    fn fingerprint(&self) -> Hash {
        Hash::of(&self.sources)
    }
}

/// The project's file loader: generated `@baudelaire/*` modules first,
/// everything else from the filesystem through typst-kit's own loader.
pub(super) struct Files {
    modules: Modules,
    system: SystemFiles,
}

impl Files {
    pub(super) fn new(cx: &ModuleCx, project: FsRoot, packages: SystemPackages) -> Self {
        Self {
            modules: Modules::new(cx),
            system: SystemFiles::new(project, packages),
        }
    }

    /// The filesystem path of `id`.
    ///
    /// A generated module has none, and must short-circuit: the system loader
    /// would try to *download* `@baudelaire/html` from the package registry.
    pub(super) fn resolve(&self, id: FileId) -> FileResult<PathBuf> {
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
}

impl FileLoader for Files {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match Modules::owner(&id) {
            Some(spec) => self.modules.load(spec, id.vpath()),
            None => self.system.load(id),
        }
    }
}

/// `@baudelaire/html`: element construction without `html.elem`'s ceremony, and
/// `svg()`, which inlines an SVG file as real DOM.
///
/// Everything but `svg()` is pure Typst that a template could have written
/// itself; it ships here so every site gets it. `svg()` leaves the markers
/// below for [`crate::render`] to resolve.
pub(crate) struct Html;

impl Html {
    /// The transient attribute `svg()` leaves on the element, naming the file
    /// to inline. Removed when [`crate::render`] splices the file in, so it
    /// never reaches the output. Bound into the module source rather than
    /// written into `typ/html.typ`, so the name lives in one place across both
    /// sides of the marker.
    pub(crate) const MARKER: &'static str = "data-baudelaire-svg";
}

impl Module for Html {
    fn name(&self) -> &'static str {
        "html"
    }

    fn source(&self, _cx: &ModuleCx) -> String {
        format!(
            "#let _svg-marker = \"{}\"\n{}",
            Html::MARKER,
            include_str!("typ/html.typ")
        )
    }
}

/// `@baudelaire/site`: site identity and build version as typed bindings, so a
/// template writes `#import "@baudelaire/site": title` instead of
/// `sys.inputs.at("baudelaire", default: (:)).at("site", ..).at("title", ..)`.
/// A typo becomes an import error rather than a silent `none`.
///
/// Config-derived values only. `git` and `date` stay on `sys.inputs`; see the
/// module-level note on why nothing volatile may be baked in.
struct Site;

impl Module for Site {
    fn name(&self) -> &'static str {
        "site"
    }

    fn source(&self, cx: &ModuleCx) -> String {
        let mut out = String::from(include_str!("typ/site.typ"));
        // The build context's `site` sub-tree plus its `version`: the same
        // value that feeds `sys.inputs`, not a second derivation from config.
        let mut fields = match cx.context.get("site") {
            Some(Value::Dict(pairs)) => pairs.clone(),
            _ => Vec::new(),
        };
        if let Some(version) = cx.context.get("version") {
            fields.insert(0, ("version".to_owned(), version.clone()));
        }
        for (name, value) in &fields {
            out.push_str(&format!("#let {name} = {}\n", Typst(value)));
        }
        out
    }
}
