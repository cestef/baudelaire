//! Where a theme's files come from.
//!
//! A theme is files, and until this existed there was one place to get them:
//! the four carried inside the binary. Everything after the fetch was already
//! general (write them into the project, record which bytes were ours, keep
//! your edits on the next update), so the only thing that had to become plural
//! is the fetch itself.
//!
//! One [`Source`] impl per kind of origin, one line in [`builtin`]. A source
//! answers two questions and nothing else: whether it recognises what the run
//! asked for, and how to produce the files. It never touches the project.

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

/// One place a theme can be fetched from.
///
/// The two methods are the two halves of the same knowledge: what a spec of
/// this kind looks like, and what to do with one. Keeping them on one impl is
/// what stops `add` accepting a spec that `update` cannot go back for.
pub trait Source {
    /// The word this source is known by: in the lock, and in diagnostics that
    /// name where a copy came from.
    fn name(&self) -> &'static str;

    /// The origin `spec` names, or `None` if this is not that kind of spec.
    /// The order in [`builtin`] settles a spec two sources would claim.
    fn parse(&self, spec: &str) -> Option<Origin>;

    /// Whether `origin` is one of this source's, which is how `update` finds
    /// the source that wrote a copy without re-reading its spec.
    fn owns(&self, origin: &Origin) -> bool;

    /// The theme's files. Only called with an origin this source [`owns`].
    ///
    /// [`owns`]: Source::owns
    fn fetch(&self, origin: &Origin, cx: &Fetching) -> Result<Fetched>;
}

/// The registered sources, in the order a spec is offered to them.
///
/// THE list: `theme add` resolves a spec through it, and `theme update` finds
/// the source that wrote a copy through it, so a source cannot install a theme
/// it could not later bring forward.
pub fn builtin() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(Shelf),
        Box::new(Store),
        // Before the forge source, whose URLs it would otherwise claim: the two
        // are spelled alike and only the archive suffix separates them.
        Box::new(Archive),
        Box::new(Repository),
        // Last: it claims anything that exists on disk.
        Box::new(Local),
    ]
}

/// What a fetch may need from the project it is fetching into.
///
/// Config the *site* set, never the source's own business: a mirror is where
/// this machine gets packages from, and a source has no opinion about that. It
/// travels as one value so a new setting reaches every source at once.
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

/// Where an installed theme came from, and everything `update` needs to go back
/// for it.
///
/// It is written into the [`Lock`](super::Lock), so a copy carries its own
/// origin: nothing has to be re-derived from a config line that may since have
/// changed, and a theme fetched from anywhere updates from the same anywhere.
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
    /// An archive at a URL, taken as it is now on every update: what the URL
    /// points at is the URL's business.
    Archive {
        url: String,
        /// The directory inside it that holds the theme, when it holds more
        /// than one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subdir: Option<PathBuf>,
    },
    /// A repository on a forge, fetched as that forge's source archive.
    ///
    /// The ref and what it resolved to are two different questions: the ref is
    /// what an update goes back to (a branch moves, and following it is the
    /// point of naming one), and `resolved` is what the forge actually served,
    /// which is the commit for a tag or a commit and the branch's own name for
    /// a branch.
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
    /// The source that owns this origin, or an error naming what wrote the
    /// record: a lock from a newer baudelaire can name a source this one does
    /// not have, and that is worth saying plainly rather than as "unknown
    /// theme".
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

    /// The theme this origin names: what its source fetches, narrowed to the
    /// directory inside it that holds the theme when one was named.
    ///
    /// The narrowing is here rather than in each source because it is the same
    /// question for all of them (a repository or an archive can hold a whole
    /// project, with the theme one directory down), and because an update has
    /// to apply exactly what the install did.
    pub fn fetch(&self, cx: &Fetching) -> Result<Fetched> {
        let fetched = self.source()?.fetch(self, cx)?;
        match self.subdir() {
            Some(subdir) => fetched.within(subdir),
            None => Ok(fetched),
        }
    }

    /// The directory inside the source that holds the theme, for the sources
    /// that can carry more than one.
    fn subdir(&self) -> Option<&Path> {
        match self {
            Self::Archive { subdir, .. } | Self::Forge { subdir, .. } => subdir.as_deref(),
            Self::Bundled { .. } | Self::Path { .. } | Self::Package { .. } => None,
        }
    }

    /// The same origin, with the theme's directory inside the source named.
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

    /// The theme `spec` asks for, whoever answers for it.
    ///
    /// A bare word nothing claimed was meant to be one of the shipped names, so
    /// it is answered with the nearest of those rather than with "no source
    /// recognises this": `albatross` is a typo, not a repository.
    pub fn parse(spec: &str) -> Result<Self> {
        if let Some(origin) = builtin().iter().find_map(|source| source.parse(spec)) {
            return Ok(origin);
        }
        match spec.contains(['/', ':', '@', '.']) {
            true => Err(ThemeError::unsupported(spec.to_owned()).into()),
            false => Err(super::Bundled::find(spec)
                .err()
                .unwrap_or_else(|| ThemeError::unsupported(spec.to_owned()).into())),
        }
    }
}

/// A theme's files, fetched and not yet written anywhere.
///
/// Held in memory rather than staged on disk: a theme is small (the shipped
/// ones are about 60 KiB), and a staging directory is a second place for a
/// half-written copy to be left behind.
pub struct Fetched {
    /// The name the copy is known by: the directory it lands in, and what the
    /// run prints.
    pub name: String,
    /// One line about the theme, when the source knows one.
    pub about: Option<String>,
    /// What to go back to, recorded in the lock.
    pub origin: Origin,
    /// Relative path to contents. Ordered, so everything derived from it (what
    /// an install writes, what a lock records, what a report lists) reads the
    /// same way twice.
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

impl Fetched {
    /// Whether the theme carries `templates/<file>`, the one question the
    /// scaffold asks of a theme it has not installed yet.
    pub fn has(&self, rel: &Path) -> bool {
        self.files.contains_key(rel)
    }

    /// The paths this theme carries, relative, in order: the shape every report
    /// and every install loop walks.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }

    /// One of the theme's files as text, when it has it and it is text: the
    /// `theme.kdl` a report reads without writing anything to disk first.
    pub fn text(&self, rel: &Path) -> Option<String> {
        String::from_utf8(self.files.get(rel)?.clone()).ok()
    }

    /// The theme inside a source that carries more than one: everything under
    /// `subdir`, with that prefix dropped, so the copy is a theme rather than a
    /// project with a theme in it.
    ///
    /// The copy is named after the directory, not after the repository: a
    /// monorepo's `themes/plume` is `plume`, and calling it after the project
    /// would put every theme it holds in the same place.
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
