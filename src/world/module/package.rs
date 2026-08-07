//! The generated modules as ordinary typst packages: what the `@baudelaire/*`
//! half of [`crate::mirror`] writes to disk.
//!
//! This module knows typst's package *layout* and nothing about where a mirror
//! puts it or what it reports; the mirror knows those and nothing about the
//! layout. A build never reads what is written either way: [`super::Files`]
//! answers this namespace from memory before typst's package resolution runs,
//! so an installed copy that is stale, edited or absent can mislead an editor
//! and can never change a page. That is what makes installing safe: it is a
//! mirror, not a source.

use std::path::{Path, PathBuf};

use typst_kit::packages::FsPackages;

use crate::codegen::Value;
use crate::config::Config;
use crate::generated::{File, Generated};
use crate::world::BuildContext;

use super::{ENTRYPOINT, Entrypoint, MANIFEST, Modules, NAMESPACE, VERSION};

/// One generated module, as the files typst's own resolution looks for.
///
/// The file-backed pair is the reason a mirror is a copy rather than a symlink:
/// their tables are derived from the site that was last built, so a second
/// project on the same machine overwrites the values, never the API.
pub struct Package {
    /// The name it is imported under: `@baudelaire/<name>`.
    pub name: &'static str,
    /// Whether the module's data came from a real build, or is the empty table
    /// a project that has never been built has to make do with.
    pub empty: bool,
    manifest: String,
    entrypoint: String,
}

impl Package {
    /// How a template names this module: `@baudelaire/<name>`. Spelled here,
    /// where the namespace is, rather than by whoever reports an install.
    pub fn specifier(&self) -> String {
        format!("@{NAMESPACE}/{}", self.name)
    }

    /// This package's directory under a package directory, which is the layout
    /// typst looks in: `<namespace>/<name>/<version>/`.
    fn directory(&self) -> PathBuf {
        Packages::namespace(Path::new(""))
            .join(self.name)
            .join(VERSION.to_string())
    }
}

impl Generated for Package {
    fn files(&self) -> Vec<File> {
        let dir = self.directory();
        vec![
            File::new(dir.join(MANIFEST), self.manifest.clone()),
            File::new(dir.join(ENTRYPOINT), self.entrypoint.clone()),
        ]
    }
}

/// The generated modules of one project, ready to be written as packages.
pub struct Packages {
    modules: Modules,
}

impl Packages {
    /// Where the packages go inside a project, relative to its root: beside the
    /// generated tables and the TypeScript declarations, under
    /// [`Config::SCRATCH`].
    ///
    /// The default, because three of the four modules are derived from *this*
    /// project (`site` from its config, `sections` and `pages` from its pages).
    /// One machine-global copy of those is one project's data shown to every
    /// other project's editor.
    pub fn project() -> PathBuf {
        Config::scratch("generated").join("packages")
    }

    /// The directory typst resolves a non-`preview` package from with nothing
    /// configured, which is what `--global` asks for. Asked of typst-kit rather
    /// than spelled out here, so it lands wherever typst itself would look.
    ///
    /// `None` when the platform has no data directory: there is then nowhere
    /// zero-config to write to, and the caller has to name one.
    pub fn directory() -> Option<PathBuf> {
        FsPackages::system_data().map(|packages| packages.path().to_path_buf())
    }

    /// Everything a mirror owns inside a package directory: one namespace
    /// directory and nothing else. What an install writes into and what an
    /// uninstall removes, stated once so the two can never disagree about the
    /// boundary. Anything above it belongs to whoever else puts packages there.
    pub fn namespace(dir: &Path) -> PathBuf {
        dir.join(NAMESPACE)
    }

    /// The modules as this project would serve them.
    pub fn new(config: &Config) -> Self {
        let root = crate::fs::canonical(&config.root);
        let tree = Value::from(&BuildContext::of(config));
        Self {
            modules: Modules::new(
                &super::ModuleCx {
                    context: &tree,
                    #[cfg(feature = "markdown")]
                    markdown: &config.content.markdown,
                },
                &root,
            ),
        }
    }

    /// Every module as a package, in the order the registry holds them.
    pub fn packages(&self) -> Vec<Package> {
        self.modules
            .entrypoints
            .iter()
            .map(|(name, entrypoint)| {
                let (entrypoint, empty) = Self::source(name, entrypoint);
                Package {
                    name,
                    empty,
                    manifest: Modules::manifest(name),
                    entrypoint,
                }
            })
            .collect()
    }

    /// A module's entrypoint source, and whether it is the empty stand-in.
    ///
    /// A file-backed table is written by the last build. It is absent in a
    /// project that has never been built, which is exactly the moment `init`
    /// mirrors, so the empty table stands in: every symbol resolves, and the
    /// next mirror after a build fills in the values.
    fn source(name: &'static str, entrypoint: &Entrypoint) -> (String, bool) {
        match entrypoint {
            Entrypoint::Memory(bytes) => (String::from_utf8_lossy(bytes).into_owned(), false),
            Entrypoint::File(path) => match std::fs::read_to_string(path) {
                Ok(source) => (source, false),
                Err(_) => (super::Table::empty(name).source(), true),
            },
        }
    }
}
