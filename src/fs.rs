//! Filesystem facade over `std::fs` that attaches path + operation context to
//! every error (see [`crate::error::FsError`]). Prefer these over `std::fs`
//! wherever a failure should tell the user *which* file and *what* operation.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::error::fs::FsError;
use crate::error::{Op, Result};

/// A relative path that cannot reach outside the tree it is joined to: the one
/// place "this must not escape the project" is decided.
///
/// A theme directory, an inlined SVG's path and a generated file's name all come
/// from text a config file or a template wrote, so each is a way to name a file
/// the project does not own. Each site used to spell its own variant of the same
/// component walk, which is the worst possible shape for a check whose whole job
/// is to be identical everywhere.
///
/// Contained means: at least one component, and every component an ordinary
/// name. No root, no drive prefix, no `..`, no leading `.`. Callers that accept
/// a project-absolute spelling (`/assets/x.svg`) strip the leading `/`
/// themselves and hand over what is left, so the leading slash is a caller's
/// syntax rather than a hole here.
///
/// The test is lexical, and deliberately so: these paths need not exist yet, so
/// nothing here touches the filesystem. A component that happens to be a symlink
/// out of the tree is judged by its name; a caller that must also refuse those
/// has to canonicalize after joining.
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
                // `..`, `.`, a root, or a `C:` prefix: each either climbs out of
                // the tree or discards the root the path is joined to.
                _ => return None,
            }
        }
        // An empty path names the root itself, which is not a file inside it.
        named.then_some(Self(path))
    }

    /// The relative path, as written.
    pub fn path(&self) -> &'a Path {
        self.0
    }

    /// This path resolved under `root`, which it is now known to stay inside.
    pub fn under(&self, root: impl AsRef<Path>) -> PathBuf {
        root.as_ref().join(self.0)
    }
}

/// Read a file to a string.
pub fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|e| FsError::new(Op::Read, path, e).into())
}

/// Read a file to bytes.
pub fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    std::fs::read(path).map_err(|e| FsError::new(Op::Read, path, e).into())
}

/// Write bytes to a file, creating it if needed.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    std::fs::write(path, contents).map_err(|e| FsError::new(Op::Write, path, e).into())
}

/// Write bytes to a file, creating any missing parent directories first: the
/// one shared "emit an output file" path.
pub fn write_all(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    write(path, contents)
}

/// Resolve a path to its canonical, absolute form.
pub fn canonicalize(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    std::fs::canonicalize(path).map_err(|e| FsError::new(Op::Canonicalize, path, e).into())
}

/// Canonicalize best-effort: the canonical path when resolvable, else the
/// lexical path unchanged. For sites where an unresolvable path must not fail
/// (dependency capture, display, watch filters).
pub fn canonical(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Remove a file.
pub fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::remove_file(path).map_err(|e| FsError::new(Op::Remove, path, e).into())
}

/// Rename a file, naming both source and destination on failure.
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    std::fs::rename(from, to).map_err(|e| FsError::between(Op::Rename, from, to, e).into())
}

/// Recursively create a directory and all its parents.
pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path).map_err(|e| FsError::new(Op::CreateDir, path, e).into())
}

/// List a directory's immediate entries as paths, sorted by path. The open *and*
/// each entry are context-wrapped, so a mid-iteration failure still names the
/// directory.
///
/// The sort is load-bearing: the OS returns entries in an arbitrary order that
/// differs between machines and shifts as a directory is edited, and that order
/// otherwise reaches feeds, listings, pagination membership and the layout
/// wrapper text, making output nondeterministic and refingerprinting pages that
/// did not change.
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

/// Copy a file, reporting the source path on failure.
pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| FsError::new(Op::Copy, from, e).into())
}

/// Recursively remove a directory and its contents.
pub fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::remove_dir_all(path).map_err(|e| FsError::new(Op::Remove, path, e).into())
}

/// A recursive walk of a directory tree: the one implementation behind "every
/// file under here", shared by content discovery, the asset and static
/// pipelines, embed fingerprinting, deploy scanning and the `dist` prune.
///
/// Symlinked directories are followed, but each directory is entered at most
/// once by canonical path, so a link pointing at an ancestor (`ln -s .
/// content/loop`) ends that branch instead of recursing until the stack
/// overflows. Release builds are `panic = "abort"` and stripped, so that
/// overflow would be a bare SIGSEGV with no diagnostic at all.
///
/// Yielded paths are `root`-joined, never canonicalized, so they keep the
/// spelling the caller asked for and always strip back to a relative path.
pub struct Walk<'a> {
    root: &'a Path,
    skip: Option<Skip<'a>>,
}

/// A predicate over directories a walk must not enter: see [`Walk::skipping`].
type Skip<'a> = Box<dyn Fn(&Path) -> bool + 'a>;

/// What a walk does with a directory it cannot read.
enum OnError {
    /// Fail the whole walk.
    Fail,
    /// End that branch and keep going.
    Prune,
}

/// The result of a [`Walk`].
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

    /// Do not enter directories for which `skip` holds, nor list their
    /// contents: a subtree owned by another stage, or one that escapes the tree
    /// being walked. Skipped directories are absent from the result entirely.
    #[must_use]
    pub fn skipping(mut self, skip: impl Fn(&Path) -> bool + 'a) -> Self {
        self.skip = Some(Box::new(skip));
        self
    }

    /// Walk the tree, failing if any directory cannot be read.
    pub fn tree(&self) -> Result<Tree> {
        self.collect(OnError::Fail)
    }

    /// Every file under the root.
    pub fn files(&self) -> Result<Vec<PathBuf>> {
        Ok(self.tree()?.files)
    }

    /// Walk best-effort: an unreadable directory contributes nothing instead of
    /// failing the walk. For callers whose result is advisory (a fingerprint of
    /// whatever is readable) rather than an output the build depends on.
    pub fn lossy(&self) -> Tree {
        self.collect(OnError::Prune).unwrap_or_default()
    }

    fn collect(&self, on_error: OnError) -> Result<Tree> {
        let mut tree = Tree::default();
        let mut seen = BTreeSet::from([canonical(self.root)]);
        self.descend(self.root, &on_error, &mut seen, &mut tree)?;
        Ok(tree)
    }

    fn descend(
        &self,
        dir: &Path,
        on_error: &OnError,
        seen: &mut BTreeSet<PathBuf>,
        tree: &mut Tree,
    ) -> Result<()> {
        let entries = match (read_dir(dir), on_error) {
            (Ok(entries), _) => entries,
            (Err(e), OnError::Fail) => return Err(e),
            (Err(_), OnError::Prune) => return Ok(()),
        };
        for path in entries {
            if !path.is_dir() {
                tree.files.push(path);
            } else if self.enters(&path, seen) {
                self.descend(&path, on_error, seen, tree)?;
                tree.dirs.push(path);
            }
        }
        Ok(())
    }

    /// Whether to descend into `dir`: not skipped, and not already visited by
    /// canonical path (the cycle guard).
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

    /// Every shape that would resolve somewhere other than under the root: the
    /// traversals, the absolute spellings, and the empty path, which names the
    /// root itself rather than a file in it.
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

    /// A drive prefix discards the root it would be joined to, so it is refused
    /// where the platform recognises one.
    #[test]
    #[cfg(windows)]
    fn contained_rejects_a_drive_prefix() {
        for path in [r"C:\Windows", r"C:x", r"\\server\share"] {
            assert!(Contained::new(path).is_none(), "{path:?} should be refused");
        }
    }

    /// Containment is lexical: a component that happens to be a symlink out of
    /// the tree passes, because these paths need not exist yet. Anything that
    /// must refuse those has to canonicalize after joining, which is a different
    /// check and a deliberate one.
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

    /// A symlinked directory pointing at an ancestor used to recurse until the
    /// stack overflowed, which release builds report as a bare SIGSEGV.
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

    /// A skipped directory is neither entered nor reported, so a caller that
    /// deletes what the walk returns cannot reach into it.
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

    /// Directories come back deepest-first, so removing them in order drops a
    /// parent only once its children are gone.
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
    fn error_names_the_path_and_operation() {
        let err = super::read_to_string("does/not/exist.typ").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("failed to read"), "{msg}");
        assert!(msg.contains("does/not/exist.typ"), "{msg}");
    }
}
