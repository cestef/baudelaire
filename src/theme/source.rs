//! Where a theme's files come from: one [`Source`] impl per kind of origin, one
//! line in [`builtin`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::archive::Archive;
use super::bundled::Shelf;
use super::forge::Repository;
use super::local::Local;
use super::package::Store;
use crate::config::Config;
use crate::error::{Result, ThemeError};

/// One place a theme can be fetched from; parsing a spec and fetching it stay
/// on one impl, so `add` cannot accept a spec `update` cannot go back for.
pub trait Source {
    /// The word this source is known by, in the lock and in diagnostics.
    fn name(&self) -> &'static str;

    /// `None` if this is not that kind of spec; the order in [`builtin`]
    /// settles a spec two sources would claim.
    fn parse(&self, spec: &str) -> Option<Origin>;

    /// How `update` finds the source that wrote a copy.
    fn owns(&self, origin: &Origin) -> bool;

    /// Only called with an origin this source [`owns`](Source::owns).
    fn fetch(&self, origin: &Origin, cx: &Fetching) -> Result<Fetched>;
}

/// The registered sources, in the order a spec is offered to them: [`Archive`]
/// before [`Repository`], whose URLs are spelled alike, and [`Local`] last,
/// since it claims anything on disk.
pub fn builtin() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(Shelf),
        Box::new(Store),
        Box::new(Archive),
        Box::new(Repository),
        Box::new(Local),
    ]
}

/// What a fetch may need from the project it is fetching into, as one value so
/// a new setting reaches every source at once.
#[derive(Debug, Default, Clone)]
pub struct Fetching {
    /// `typst { registry }`: the mirror package downloads are redirected to.
    pub registry: Option<String>,
}

impl From<&Config> for Fetching {
    fn from(config: &Config) -> Self {
        Self {
            registry: config.typst.registry.clone(),
        }
    }
}

/// Where an installed theme came from, written into the [`Lock`](super::Lock)
/// so a copy carries its own origin rather than re-deriving one from a config
/// line that may since have changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "kebab-case")]
pub enum Origin {
    /// A theme this binary carries.
    Bundled { name: String },
    /// A directory on this machine. Absolute, because the record naming it is
    /// read from wherever the next run happens to be.
    Path { path: PathBuf },
    /// A Typst package, by the specifier the compiler would resolve.
    Package { spec: String },
    /// An archive at a URL, taken as it is now on every update.
    Archive {
        url: String,
        /// The directory inside it that holds the theme, when it holds more
        /// than one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subdir: Option<PathBuf>,
    },
    /// `ref` is what an update goes back to, so a branch still moves;
    /// `resolved` is what the forge actually served last time.
    Forge {
        repo: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r#ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolved: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subdir: Option<PathBuf>,
    },
}

impl Origin {
    /// The source that owns this origin, or an error naming it: a lock from a
    /// newer baudelaire can name a source this one does not have.
    pub fn source(&self) -> Result<Box<dyn Source>> {
        builtin()
            .into_iter()
            .find(|source| source.owns(self))
            .ok_or_else(|| ThemeError::unsupported(self.label()).into())
    }

    /// How the origin reads in a message: the source, and what it names.
    pub fn label(&self) -> String {
        match self {
            Self::Bundled { name } => name.clone(),
            Self::Path { path } => path.display().to_string(),
            Self::Package { spec } => spec.clone(),
            Self::Archive { url, .. } => url.clone(),
            Self::Forge { repo, r#ref, .. } => match r#ref {
                Some(name) => format!("{repo}#{name}"),
                None => repo.clone(),
            },
        }
    }

    /// The source's fetch, narrowed to the named subdirectory; the narrowing is
    /// here rather than per source so an update applies what the install did.
    pub fn fetch(&self, cx: &Fetching) -> Result<Fetched> {
        let fetched = self.source()?.fetch(self, cx)?;
        match self.subdir() {
            Some(subdir) => fetched.within(subdir),
            None => Ok(fetched),
        }
    }

    /// `None` for the sources that carry only one theme.
    fn subdir(&self) -> Option<&Path> {
        match self {
            Self::Archive { subdir, .. } | Self::Forge { subdir, .. } => subdir.as_deref(),
            Self::Bundled { .. } | Self::Path { .. } | Self::Package { .. } => None,
        }
    }

    #[must_use]
    pub fn within(self, subdir: Option<PathBuf>) -> Self {
        match self {
            Self::Archive { url, .. } => Self::Archive { url, subdir },
            Self::Forge {
                repo,
                r#ref,
                resolved,
                ..
            } => Self::Forge {
                repo,
                r#ref,
                resolved,
                subdir,
            },
            other => other,
        }
    }

    /// A bare word nothing claimed is answered with the nearest shipped name:
    /// `albatross` is a typo, not a repository.
    pub fn parse(spec: &str) -> Result<Self> {
        if let Some(origin) = builtin().iter().find_map(|source| source.parse(spec)) {
            return Ok(origin);
        }
        if spec.contains(['/', ':', '@', '.']) {
            Err(ThemeError::unsupported(spec.to_owned()).into())
        } else {
            Err(super::Bundled::find(spec)
                .err()
                .unwrap_or_else(|| ThemeError::unsupported(spec.to_owned()).into()))
        }
    }
}

/// A theme's files, held in memory rather than staged on disk, so there is no
/// second place for a half-written copy to be left behind.
pub struct Fetched {
    /// The name the copy is known by, and the directory it lands in.
    pub name: String,
    pub about: Option<String>,
    pub origin: Origin,
    /// Relative path to contents.
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

impl Fetched {
    pub fn has(&self, rel: &Path) -> bool {
        self.files.contains_key(rel)
    }

    /// Relative, in order.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }

    /// `None` when the theme lacks the file, or it is not text.
    pub fn text(&self, rel: &Path) -> Option<String> {
        String::from_utf8(self.files.get(rel)?.clone()).ok()
    }

    /// Everything under `subdir`, with that prefix dropped; the copy is named
    /// after the directory and not the repository, or every theme a monorepo
    /// holds would land in the same place.
    pub fn within(self, subdir: &Path) -> Result<Self> {
        let files: BTreeMap<PathBuf, Vec<u8>> = self
            .files
            .into_iter()
            .filter_map(|(rel, bytes)| Some((rel.strip_prefix(subdir).ok()?.to_path_buf(), bytes)))
            .collect();
        if files.is_empty() {
            return Err(ThemeError::empty(&subdir.display().to_string()).into());
        }
        Ok(Self {
            name: subdir
                .file_name()
                .map_or(self.name, |name| name.to_string_lossy().into_owned()),
            files,
            ..self
        })
    }
}
