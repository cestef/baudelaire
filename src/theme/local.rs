//! A theme from a directory on this machine.
//!
//! The one source with nothing between it and the files: a theme you are
//! writing, one another project already has, or one you cloned yourself. It is
//! also what every fetching source reduces to once it has the bytes somewhere,
//! so the reading of a theme directory lives here and they call it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::install::Lock;
use super::source::{Fetched, Fetching, Origin, Source};
use crate::error::{Result, ThemeError};

/// A directory read as a theme.
pub struct Local;

impl Local {
    /// The prefixes that mean "a path" even when nothing is there yet, so a
    /// mistyped `./plume` is answered as a missing directory rather than as a
    /// spec no source recognises.
    const PREFIXES: [&'static str; 4] = ["./", "../", "/", "~/"];

    /// Read a directory as a theme: every file under it, keyed by its path
    /// relative to the root.
    ///
    /// Two things are left behind, and both would be wrong to carry. A `.git`
    /// directory is the source's history, not the theme's files. A lock is the
    /// *other* copy's record of what baudelaire wrote there, and copying it
    /// would have this copy claim bytes it never wrote and an origin it did not
    /// come from.
    pub fn read(
        root: &Path,
        name: String,
        about: Option<String>,
        origin: Origin,
    ) -> Result<Fetched> {
        if !root.is_dir() {
            return Err(ThemeError::missing(&root.display().to_string()).into());
        }
        let tree = crate::fs::Walk::new(root)
            .skipping(|dir| dir.file_name().is_some_and(|name| name == ".git"))
            .tree()?;
        let files: BTreeMap<PathBuf, Vec<u8>> = tree
            .files
            .iter()
            .filter_map(|path| Some((path.strip_prefix(root).ok()?.to_path_buf(), path)))
            .filter(|(rel, _)| rel != Path::new(Lock::FILE))
            .map(|(rel, path)| crate::fs::read(path).map(|bytes| (rel, bytes)))
            .collect::<Result<_>>()?;
        if files.is_empty() {
            return Err(ThemeError::empty(&root.display().to_string()).into());
        }
        Ok(Fetched {
            name,
            about,
            origin,
            files,
        })
    }

    /// The name a directory's theme is known by: the directory's own.
    pub fn names(path: &Path) -> Result<String> {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| ThemeError::unnamed(&path.display().to_string()).into())
    }

    /// `~` expanded, and the path made absolute, because the record it lands in
    /// is read from wherever the next run happens to be.
    fn resolve(spec: &str) -> PathBuf {
        let path = match spec.strip_prefix("~/") {
            Some(rest) => match std::env::home_dir() {
                Some(home) => home.join(rest),
                None => PathBuf::from(spec),
            },
            None => PathBuf::from(spec),
        };
        crate::fs::canonical(&path)
    }
}

impl Source for Local {
    fn name(&self) -> &'static str {
        "path"
    }

    /// A spec that says it is a path, or one that is: an existing directory is
    /// claimed however it is spelled, so `themes/plume` works without a `./`.
    fn parse(&self, spec: &str) -> Option<Origin> {
        let looks = Self::PREFIXES.iter().any(|prefix| spec.starts_with(prefix));
        let path = Self::resolve(spec);
        (looks || path.is_dir()).then_some(Origin::Path { path })
    }

    fn owns(&self, origin: &Origin) -> bool {
        matches!(origin, Origin::Path { .. })
    }

    fn fetch(&self, origin: &Origin, _cx: &Fetching) -> Result<Fetched> {
        let Origin::Path { path } = origin else {
            return Err(ThemeError::unsupported(origin.label()).into());
        };
        Self::read(path, Self::names(path)?, None, origin.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a path spec looks like: a directory that is there, or a spelling
    /// that could only be a path. A bare word is not one, or every typo would
    /// be answered as a missing directory instead of as a theme name.
    #[test]
    fn a_path_is_claimed_by_its_spelling_or_by_being_there() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("plume");
        std::fs::create_dir_all(&dir).expect("mkdir");

        assert!(Local.parse("./nowhere").is_some(), "a spelling is enough");
        assert!(Local.parse(&dir.display().to_string()).is_some());
        assert!(Local.parse("plume").is_none(), "a bare word is a name");
    }

    /// The two files a copy must not carry: the source's history, and the other
    /// copy's record of what baudelaire wrote there.
    #[test]
    fn a_read_leaves_the_history_and_the_other_copy_s_record() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("plume");
        std::fs::create_dir_all(dir.join(".git")).expect("mkdir");
        std::fs::create_dir_all(dir.join("templates")).expect("mkdir");
        std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").expect("write");
        std::fs::write(dir.join(Lock::FILE), "{}").expect("write");
        std::fs::write(dir.join("templates/page.typ"), "#let page = 1\n").expect("write");
        std::fs::write(dir.join("theme.kdl"), "lang \"fr\"\n").expect("write");

        let fetched = Local
            .fetch(&Origin::Path { path: dir }, &Fetching::default())
            .expect("read");
        let paths: Vec<String> = fetched
            .paths()
            .map(|rel| rel.display().to_string())
            .collect();
        assert_eq!(paths, ["templates/page.typ", "theme.kdl"]);
        assert_eq!(fetched.name, "plume");
    }

    /// A directory with nothing in it is not a theme, and the run says so
    /// rather than writing a record of no files and reporting success.
    #[test]
    fn an_empty_directory_is_not_a_theme() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("hollow");
        std::fs::create_dir_all(&dir).expect("mkdir");
        assert!(
            Local
                .fetch(&Origin::Path { path: dir }, &Fetching::default())
                .is_err()
        );
    }
}
