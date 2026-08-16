//! Filesystem facade over `std::fs` that attaches path and operation context to
//! every error (see [`crate::error::FsError`]).

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::error::fs::FsError;
use crate::error::{Op, Result};

/// A relative path that cannot reach outside the tree it is joined to: at least
/// one component, every component an ordinary name.
///
/// The test is lexical, so these paths need not exist yet and a component that
/// happens to be a symlink out of the tree is judged by its name; a caller that
/// must refuse those has to canonicalize after joining.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contained<'a>(&'a Path);

impl<'a> Contained<'a> {
    /// The checked constructor: `None` for anything that could leave the tree,
    /// leaving the caller to report it with its own diagnostic.
    pub fn new<P: AsRef<Path> + ?Sized>(path: &'a P) -> Option<Self> {
        let path = path.as_ref();
        let mut named = false;
        for component in path.components() {
            match component {
                Component::Normal(_) => named = true,
                _ => return None,
            }
        }
        named.then_some(Self(path))
    }

    pub fn path(&self) -> &'a Path {
        self.0
    }

    /// This path resolved under `root`, which it is now known to stay inside.
    pub fn under(&self, root: impl AsRef<Path>) -> PathBuf {
        root.as_ref().join(self.0)
    }
}

pub fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|e| FsError::new(Op::Read, path, e).into())
}

pub fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    std::fs::read(path).map_err(|e| FsError::new(Op::Read, path, e).into())
}

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    std::fs::write(path, contents).map_err(|e| FsError::new(Op::Write, path, e).into())
}

/// Write bytes to a file, creating any missing parent directories first.
pub fn write_all(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    write(path, contents)
}

pub fn canonicalize(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    std::fs::canonicalize(path).map_err(|e| FsError::new(Op::Canonicalize, path, e).into())
}

/// Canonicalize best-effort: the canonical path when resolvable, else the
/// lexical path unchanged.
pub fn canonical(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// One spelling for a path whose file need not exist: the canonical form of its
/// deepest existing ancestor, with the components below it appended as written.
///
/// Unlike [`canonical`], this answers the same before and after the file
/// appears, so anything keyed on the path still matches itself.
pub fn resolved(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => resolved(parent).join(name),
        _ => path.to_path_buf(),
    }
}

pub fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::remove_file(path).map_err(|e| FsError::new(Op::Remove, path, e).into())
}

pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    std::fs::rename(from, to).map_err(|e| FsError::between(Op::Rename, from, to, e).into())
}

pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path).map_err(|e| FsError::new(Op::CreateDir, path, e).into())
}

/// List a directory's immediate entries as paths, sorted by path.
///
/// The sort is load-bearing: the OS order differs between machines and would
/// otherwise reach feeds, listings and page fingerprints.
pub fn read_dir(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let path = path.as_ref();
    let read = std::fs::read_dir(path).map_err(|e| FsError::new(Op::ReadDir, path, e))?;
    let mut entries = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| FsError::new(Op::ReadDir, path, e))?;
        entries.push(entry.path());
    }
    entries.sort();
    Ok(entries)
}

pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| FsError::new(Op::Copy, from, e).into())
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::remove_dir_all(path).map_err(|e| FsError::new(Op::Remove, path, e).into())
}

/// A recursive walk of a directory tree.
///
/// Symlinked directories are followed, but each directory is entered at most
/// once by canonical path, so a link pointing at an ancestor ends that branch
/// instead of recursing forever. Yielded paths are `root`-joined and never
/// canonicalized, so they keep the caller's spelling.
pub struct Walk<'a> {
    root: &'a Path,
    skip: Option<Skip<'a>>,
}

/// A predicate over directories a walk must not enter: see [`Walk::skipping`].
type Skip<'a> = Box<dyn Fn(&Path) -> bool + 'a>;

#[derive(Debug, Default)]
pub struct Tree {
    /// Every file found, parents before children.
    pub files: Vec<PathBuf>,
    /// Every directory entered except the root, children before parents, so
    /// removing them in order drops a parent only after its contents.
    pub dirs: Vec<PathBuf>,
}

impl<'a> Walk<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self { root, skip: None }
    }

    /// Do not enter directories for which `skip` holds; they are absent from
    /// the result entirely, contents included.
    #[must_use]
    pub fn skipping(mut self, skip: impl Fn(&Path) -> bool + 'a) -> Self {
        self.skip = Some(Box::new(skip));
        self
    }

    /// Walk the tree, failing if any directory cannot be read.
    pub fn tree(&self) -> Result<Tree> {
        let mut tree = Tree::default();
        let mut seen = BTreeSet::from([canonical(self.root)]);
        self.descend(self.root, &mut seen, &mut tree)?;
        Ok(tree)
    }

    pub fn files(&self) -> Result<Vec<PathBuf>> {
        Ok(self.tree()?.files)
    }

    fn descend(&self, dir: &Path, seen: &mut BTreeSet<PathBuf>, tree: &mut Tree) -> Result<()> {
        for path in read_dir(dir)? {
            if !path.is_dir() {
                tree.files.push(path);
            } else if self.enters(&path, seen) {
                self.descend(&path, seen, tree)?;
                tree.dirs.push(path);
            }
        }
        Ok(())
    }

    /// Whether to descend into `dir`: not skipped, and not already visited by
    /// canonical path.
    fn enters(&self, dir: &Path, seen: &mut BTreeSet<PathBuf>) -> bool {
        !self.skip.as_ref().is_some_and(|skip| skip(dir)) && seen.insert(canonical(dir))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Contained;

    #[test]
    fn contained_accepts_an_ordinary_relative_path() {
        for path in ["x.svg", "themes/plume", "a/b/c.html", "a/./b"] {
            let rel = Contained::new(path).unwrap_or_else(|| panic!("{path} should be contained"));
            assert_eq!(rel.under("/site"), Path::new("/site").join(path));
        }
    }

    #[test]
    fn contained_rejects_anything_that_could_leave_the_tree() {
        for path in [
            "",
            ".",
            "..",
            "../x.svg",
            "a/../../x.svg",
            "./x.svg",
            "/",
            "/etc/passwd",
        ] {
            assert!(Contained::new(path).is_none(), "{path:?} should be refused");
        }
    }

    #[test]
    #[cfg(windows)]
    fn contained_rejects_a_drive_prefix() {
        for path in [r"C:\Windows", r"C:x", r"\\server\share"] {
            assert!(Contained::new(path).is_none(), "{path:?} should be refused");
        }
    }

    #[test]
    #[cfg(unix)]
    fn contained_judges_a_symlinked_component_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let outside = tmp.path().join("outside");
        super::create_dir_all(&project).unwrap();
        super::create_dir_all(&outside).unwrap();
        super::write(outside.join("secret.svg"), "").unwrap();
        std::os::unix::fs::symlink(&outside, project.join("escape")).unwrap();

        let rel = Contained::new("escape/secret.svg").expect("lexically contained");

        assert_eq!(rel.under(&project), project.join("escape/secret.svg"));
        assert!(!super::canonical(rel.under(&project)).starts_with(super::canonical(&project)));
    }

    #[test]
    #[cfg(unix)]
    fn walk_terminates_on_a_symlink_cycle() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("content");
        super::create_dir_all(root.join("posts")).unwrap();
        super::write(root.join("posts/a.typ"), "").unwrap();
        std::os::unix::fs::symlink(&root, root.join("posts/loop")).unwrap();

        let files = super::Walk::new(&root).files().unwrap();

        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].ends_with("posts/a.typ"), "{files:?}");
    }

    #[test]
    fn walk_omits_skipped_subtrees() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        super::create_dir_all(root.join("keep")).unwrap();
        super::create_dir_all(root.join("skip/nested")).unwrap();
        super::write(root.join("keep/a.html"), "").unwrap();
        super::write(root.join("skip/nested/b.html"), "").unwrap();

        let tree = super::Walk::new(root)
            .skipping(|dir: &Path| dir.ends_with("skip"))
            .tree()
            .unwrap();

        assert_eq!(tree.files, [root.join("keep/a.html")]);
        assert_eq!(tree.dirs, [root.join("keep")]);
    }

    #[test]
    fn walk_reports_directories_children_first() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        super::create_dir_all(root.join("a/b/c")).unwrap();

        let tree = super::Walk::new(root).tree().unwrap();

        assert_eq!(
            tree.dirs,
            [root.join("a/b/c"), root.join("a/b"), root.join("a")]
        );
    }

    #[test]
    fn read_dir_returns_entries_sorted() {
        let dir = std::env::temp_dir().join("baudelaire-read-dir-sorted");
        let _ = std::fs::remove_dir_all(&dir);
        super::create_dir_all(&dir).unwrap();
        for name in ["c.typ", "a.typ", "b.typ"] {
            super::write(dir.join(name), "").unwrap();
        }

        let entries = super::read_dir(&dir).unwrap();
        let names: Vec<_> = entries
            .iter()
            .filter_map(|p| p.file_name()?.to_str())
            .collect();
        assert_eq!(names, ["a.typ", "b.typ", "c.typ"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn resolved_answers_the_same_before_and_after_a_file_appears() {
        let tmp = tempfile::tempdir().unwrap();
        let root = super::canonical(tmp.path());
        super::create_dir_all(root.join("real")).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("linked")).unwrap();
        let path = root.join("linked/a.typ");

        let before = super::resolved(&path);
        super::write(root.join("real/a.typ"), "").unwrap();

        assert_eq!(before, root.join("real/a.typ"));
        assert_eq!(before, super::resolved(&path));
        assert_eq!(
            super::resolved("no/such/dir/a.typ"),
            Path::new("no/such/dir/a.typ")
        );
    }

    #[test]
    fn error_names_the_path_and_operation() {
        let err = super::read_to_string("does/not/exist.typ").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("failed to read"), "{msg}");
        assert!(msg.contains("does/not/exist.typ"), "{msg}");
    }
}
