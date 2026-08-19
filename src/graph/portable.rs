//! How a persisted manifest keys a path, for every manifest that keys one.

use std::path::{Path, PathBuf};

/// The path-key policy a cache manifest is written under, bound to the project
/// root the keys are relative to.
///
/// One policy for every manifest: the discovery cache and the compile cache
/// key the same file the same way, so moving the site warms or cools both
/// together rather than one of them.
pub struct Portable<'a>(pub &'a Path);

impl Portable<'_> {
    /// A path as a manifest stores it: relative to the project root when it
    /// lies under it (so a warm cache survives the site moving), absolute
    /// otherwise — the typst package cache is machine-global anyway. Never
    /// relative to the process's working directory, and never dependent on
    /// whether the file exists yet: one path has exactly one key.
    ///
    /// Both halves matter for a negative dependency: a probe at a page not
    /// written yet must key exactly like the page that later appears there, or
    /// the recorded `None` keeps reading as "nothing here" and the page that
    /// probed stays a hit with a broken link.
    pub fn key(&self, path: &Path) -> PathBuf {
        let resolved = crate::fs::resolved(path);
        let absolute = if resolved.is_absolute() {
            resolved
        } else {
            self.0.join(resolved)
        };
        absolute
            .strip_prefix(self.0)
            .unwrap_or(&absolute)
            .to_path_buf()
    }

    /// The inverse of [`key`](Self::key): a stored key back to a real path.
    pub fn resolve(&self, key: &Path) -> PathBuf {
        self.0.join(key)
    }
}

/// Unix-only whole: its one test keys across a symlink, and on Windows the
/// module's imports would be an unused-import error rather than a skipped test.
#[cfg(all(test, unix))]
mod tests {
    use std::path::Path;

    use super::Portable;

    /// The key spelling is a contract: root-relative under the root, absolute
    /// outside it, and the same before and after a file appears there.
    #[test]
    fn keys_do_not_depend_on_whether_the_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = crate::fs::canonical(tmp.path());
        let keys = Portable(&root);

        std::fs::write(root.join("layout.typ"), "").unwrap();
        let inside = keys.key(&root.join("layout.typ"));
        assert_eq!(inside, Path::new("layout.typ"));
        assert_eq!(keys.resolve(&inside), root.join("layout.typ"));

        let external = crate::fs::canonical(outside.path()).join("theme.typ");
        std::fs::write(&external, "").unwrap();
        let key = keys.key(&external);
        assert_eq!(key, external);
        assert_eq!(keys.resolve(&key), external);

        std::fs::create_dir_all(root.join("vault/posts")).unwrap();
        std::fs::create_dir_all(root.join("content")).unwrap();
        std::os::unix::fs::symlink(root.join("vault/posts"), root.join("content/posts")).unwrap();
        let missing = root.join("content/posts/b.typ");
        let before = keys.key(&missing);
        std::fs::write(root.join("vault/posts/b.typ"), "").unwrap();
        assert_eq!(
            before,
            keys.key(&missing),
            "a path must key the same before and after the file appears"
        );
        assert_eq!(before, Path::new("vault/posts/b.typ"));
    }
}
