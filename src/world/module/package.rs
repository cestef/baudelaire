//! The generated modules as files on disk, under `.baudelaire/site/`, where an
//! editor resolves a template's imports with nothing configured.

use std::path::PathBuf;

use crate::codegen::Value;
use crate::config::Config;
use crate::generated::{File, Generated};
use crate::world::BuildContext;

use super::{Entrypoint, Modules};

/// One generated module, as the file an editor opens.
pub struct Package {
    pub name: &'static str,
    /// Whether the module's data came from a real build, or is the empty
    /// stand-in table.
    pub empty: bool,
    source: String,
}

impl Package {
    /// How a template imports this module: `/.baudelaire/site/<name>.typ`.
    pub fn specifier(&self) -> String {
        Modules::import(self.name)
    }
}

impl Generated for Package {
    fn files(&self) -> Vec<File> {
        vec![File::new(Modules::path(self.name), self.source.clone())]
    }
}

/// The generated modules of one project, ready to be written under its root.
///
/// A build serves the in-memory modules from memory, so what is written is a
/// copy and never a source: a stale or absent one can mislead an editor and can
/// never change a page.
pub struct Mirrored {
    modules: Modules,
}

impl Mirrored {
    /// The directory the modules are written to, relative to the project root.
    pub fn directory() -> PathBuf {
        Config::scratch(crate::config::Scratch::Site)
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

    /// Every module, in the order the registry holds them.
    pub fn packages(&self) -> Vec<Package> {
        self.modules.entrypoints.iter().map(Self::package).collect()
    }

    /// The in-memory modules alone, which is what a build writes: the
    /// file-backed tables are written by the build itself, at the same path.
    pub fn memory(&self) -> Vec<Package> {
        self.modules
            .entrypoints
            .iter()
            .filter(|(_, entrypoint)| matches!(entrypoint, Entrypoint::Memory(_)))
            .map(Self::package)
            .collect()
    }

    fn package((name, entrypoint): (&&'static str, &Entrypoint)) -> Package {
        let (source, empty) = Self::source(name, entrypoint);
        Package {
            name,
            empty,
            source,
        }
    }

    /// A module's source, and whether it is the empty stand-in, which is what a
    /// project that has never been built has instead of the table a build
    /// writes.
    fn source(name: &'static str, entrypoint: &Entrypoint) -> (String, bool) {
        match entrypoint {
            Entrypoint::Memory(bytes) => (String::from_utf8_lossy(bytes).into_owned(), false),
            Entrypoint::File(path) => std::fs::read_to_string(path).map_or_else(
                |_| (super::Table::empty(name).source(), true),
                |source| (source, false),
            ),
        }
    }
}
