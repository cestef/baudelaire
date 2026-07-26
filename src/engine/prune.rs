//! Remove orphaned outputs from `dist`: files a previous build wrote that the
//! current one no longer produces: a deleted page, a renamed permalink, a
//! removed taxonomy term or paginated index, a redirect that was taken down.
//!
//! Without this pass `dist` only ever grows: a stale file lingers and keeps
//! serving content no source maps to (e.g. an old `og:url` after the site URL
//! changed). The asset subtree is exempt (the pipeline already wipes and
//! regenerates it wholesale, [`crate::engine::asset`]), as is the build cache,
//! which lives outside `dist` but is skipped defensively in case it is nested.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::fs;

/// Deletes files under `dist` that are not in the produced set.
pub struct Prune<'a> {
    dist: &'a Path,
    /// `dist` on disk, to test containment against: the one boundary the sweep
    /// may not delete outside of.
    root: PathBuf,
    /// Directory prefixes owned by another stage (the asset pipeline) or outside
    /// the build (the cache): never walked, never pruned.
    protected: Vec<PathBuf>,
}

impl<'a> Prune<'a> {
    /// Prune `dist`, keeping the asset tree and the build cache untouched.
    pub fn new(dist: &'a Path, asset_dist: &Path, cache: &Path) -> Self {
        let protected = [asset_dist, cache].into_iter().map(fs::canonical).collect();
        Self {
            dist,
            root: fs::canonical(dist),
            protected,
        }
    }

    /// Delete every file under `dist` whose canonical path is not in `keep`,
    /// then drop any directory left empty. `keep` holds the outputs the current
    /// build produced (page HTML, static passthrough, generated files); it is
    /// canonicalized here so it compares equal to the walked paths regardless of
    /// how each was spelled. Returns the number of files removed.
    pub fn run(&self, keep: &[PathBuf]) -> Result<usize> {
        if !self.owns(self.dist) {
            return Ok(0);
        }
        let keep: BTreeSet<PathBuf> = keep.iter().map(fs::canonical).collect();
        let tree = fs::Walk::new(self.dist)
            .skipping(|dir| !self.owns(dir))
            .tree()?;
        let mut removed = 0;
        for file in &tree.files {
            if !keep.contains(&fs::canonical(file)) {
                fs::remove_file(file)?;
                removed += 1;
            }
        }
        // `dirs` comes back children-first, so a directory the sweep emptied is
        // dropped after its contents; a still-populated one (kept files, or a
        // skipped subtree) errors and is ignored.
        for dir in &tree.dirs {
            let _ = std::fs::remove_dir(dir);
        }
        Ok(removed)
    }

    /// Whether the sweep may enter `dir`: it resolves inside `dist` and sits
    /// outside every protected subtree. Symlinks are followed, so containment is
    /// what stops `ln -s ~/docs dist/docs` deleting outside the project.
    fn owns(&self, dir: &Path) -> bool {
        let canon = fs::canonical(dir);
        canon.starts_with(&self.root) && !self.protected.iter().any(|p| canon.starts_with(p))
    }
}

#[cfg(test)]
mod tests {
    use super::Prune;
    use std::fs;
    use std::path::PathBuf;

    /// Build a `dist` tree from relative paths and return its root (tempdir kept
    /// alive by the returned guard).
    fn dist(files: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("dist");
        for rel in files {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"x").unwrap();
        }
        (tmp, root)
    }

    #[test]
    fn removes_orphans_and_keeps_the_rest() {
        let (_g, root) = dist(&["a/index.html", "b/index.html"]);
        let keep = vec![root.join("a/index.html")];
        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"))
            .run(&keep)
            .unwrap();
        assert_eq!(removed, 1);
        assert!(root.join("a/index.html").exists());
        assert!(!root.join("b/index.html").exists());
        // The emptied directory is swept away, not left as a husk.
        assert!(!root.join("b").exists());
    }

    #[test]
    fn leaves_the_asset_subtree_untouched() {
        // The asset pipeline owns `assets/` and regenerates it wholesale, so the
        // prune must not touch a file there even when it is absent from `keep`.
        let (_g, root) = dist(&["page/index.html", "assets/app.abc123.js"]);
        let keep = vec![root.join("page/index.html")];
        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"))
            .run(&keep)
            .unwrap();
        assert_eq!(removed, 0);
        assert!(root.join("assets/app.abc123.js").exists());
    }

    /// A symlinked directory inside `dist` pointing elsewhere used to be swept
    /// like any other: `ln -s ~/docs dist/docs` deleted files outside the
    /// project entirely.
    #[test]
    #[cfg(unix)]
    fn does_not_sweep_through_a_symlink_out_of_dist() {
        let (_g, root) = dist(&["a/index.html"]);
        let outside = root.parent().unwrap().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"x").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked")).unwrap();

        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"))
            .run(&[root.join("a/index.html")])
            .unwrap();

        assert_eq!(removed, 0);
        assert!(outside.join("secret.txt").exists());
    }

    #[test]
    fn an_empty_keep_set_clears_everything_but_protected() {
        let (_g, root) = dist(&["x.html", "d/y.html", "assets/a.js"]);
        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"))
            .run(&[])
            .unwrap();
        assert_eq!(removed, 2);
        assert!(root.join("assets/a.js").exists());
        assert!(!root.join("x.html").exists());
        assert!(!root.join("d").exists());
    }
}
