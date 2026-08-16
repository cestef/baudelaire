//! The generated modules as ordinary typst packages: what the `@baudelaire/*`
//! half of [`crate::mirror`] writes to disk.

use std::path::{Path, PathBuf};

use typst_kit::packages::FsPackages;

use crate::codegen::Value;
use crate::config::Config;
use crate::generated::{File, Generated};
use crate::world::BuildContext;

use super::{ENTRYPOINT, Entrypoint, MANIFEST, Modules, NAMESPACE, VERSION};

/// One generated module, as the files typst's own resolution looks for.
pub struct Package {
    /// The name it is imported under: `@baudelaire/<name>`.
    pub name: &'static str,
    /// Whether the module's data came from a real build, or is the empty
    /// stand-in table.
    pub empty: bool,
    manifest: String,
    entrypoint: String,
}

impl Package {
    /// How a template names this module: `@baudelaire/<name>`.
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
///
/// A build answers this namespace from memory, so what is written is a mirror
/// and never a source: an installed copy that is stale or absent can mislead
/// an editor and can never change a page.
pub struct Packages {
    modules: Modules,
}

impl Packages {
    /// Where the packages go inside a project, relative to its root, and the
    /// default, most modules being derived from that project.
    pub fn project() -> PathBuf {
        Config::scratch(crate::config::Scratch::Generated).join("packages")
    }

    /// The directory typst resolves a non-`preview` package from with nothing
    /// configured, which is what `--global` asks for, or `None` when the
    /// platform has no data directory and the caller has to name one.
    pub fn directory() -> Option<PathBuf> {
        FsPackages::system_data().map(|packages| packages.path().to_path_buf())
    }

    /// Everything a mirror owns inside a package directory: one namespace
    /// directory and nothing else, which is what an uninstall removes.
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
                    sources: &config.paths.sources,
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

    /// A module's entrypoint source, and whether it is the empty stand-in,
    /// which is what a project that has never been built has instead of the
    /// table a build writes.
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
