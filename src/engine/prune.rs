//! Remove orphaned outputs from `dist`: files a previous build wrote that the
//! current one no longer produces. The asset subtree, which its pipeline
//! regenerates wholesale, and the build cache are exempt.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use wax::{Glob, Program};

use crate::error::{ContentError, Result};
use crate::fs;

/// Deletes files under `dist` that are not in the produced set. Containment is
/// the only thing bounding it; that `dist` holds no source tree is established
/// once, by [`Paths::swallowed`](crate::config::Paths::swallowed).
pub struct Prune<'a> {
    dist: &'a Path,
    /// `dist` on disk, to test containment against: the one boundary the sweep
    /// may not delete outside of.
    root: PathBuf,
    /// Directory prefixes owned by another stage (the asset pipeline) or outside
    /// the build (the cache): never walked, never pruned.
    protected: Vec<PathBuf>,
    /// `prune { keep }`: globs, relative to `dist`, whose matches survive
    /// whether or not this build produced them. Files only; a directory holding
    /// one is left standing by the empty-directory sweep.
    spared: Vec<Glob<'static>>,
}

impl<'a> Prune<'a> {
    /// Prune `dist`, keeping the asset tree, the build cache and everything
    /// `keep` matches untouched.
    pub fn new(dist: &'a Path, asset_dist: &Path, cache: &Path, keep: &[String]) -> Result<Self> {
        let protected = [asset_dist, cache].into_iter().map(fs::canonical).collect();
        let spared = keep
            .iter()
            .map(|pattern| {
                Glob::new(pattern)
                    .map(Glob::into_owned)
                    .map_err(|e| ContentError::bad_glob("prune", pattern, e).into())
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            dist,
            root: fs::canonical(dist),
            protected,
            spared,
        })
    }

    /// Delete every file under `dist` whose canonical path is not in `keep`,
    /// then drop any directory left empty (`dirs` comes back children-first, so
    /// a still-populated one simply errors and is ignored). Returns the number
    /// of files removed.
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
            if keep.contains(&fs::canonical(file)) || self.spared(file) {
                continue;
            }
            fs::remove_file(file)?;
            removed += 1;
        }
        for dir in &tree.dirs {
            let _ = std::fs::remove_dir(dir);
        }
        Ok(removed)
    }

    /// Whether `prune { keep }` claims this file, matched on its path relative
    /// to `dist`: the spelling the author wrote the glob in, and the only one
    /// stable across an absolute, relative or symlinked `dist`.
    fn spared(&self, file: &Path) -> bool {
        let Ok(rel) = file.strip_prefix(self.dist) else {
            return false;
        };
        self.spared.iter().any(|glob| glob.is_match(rel))
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

    /// Build a `dist` tree from relative paths and return its root, with the
    /// tempdir guard that keeps it alive.
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
        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"), &[])
            .unwrap()
            .run(&keep)
            .unwrap();
        assert_eq!(removed, 1);
        assert!(root.join("a/index.html").exists());
        assert!(!root.join("b/index.html").exists());
        assert!(!root.join("b").exists());
    }

    #[test]
    fn leaves_the_asset_subtree_untouched() {
        let (_g, root) = dist(&["page/index.html", "assets/app.abc123.js"]);
        let keep = vec![root.join("page/index.html")];
        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"), &[])
            .unwrap()
            .run(&keep)
            .unwrap();
        assert_eq!(removed, 0);
        assert!(root.join("assets/app.abc123.js").exists());
    }

    #[test]
    #[cfg(unix)]
    fn does_not_sweep_through_a_symlink_out_of_dist() {
        let (_g, root) = dist(&["a/index.html"]);
        let outside = root.parent().unwrap().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"x").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked")).unwrap();

        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"), &[])
            .unwrap()
            .run(&[root.join("a/index.html")])
            .unwrap();

        assert_eq!(removed, 0);
        assert!(outside.join("secret.txt").exists());
    }

    #[test]
    fn spares_what_the_keep_globs_match() {
        let (_g, root) = dist(&["a/index.html", "themes/spleen/index.html", "stale.html"]);
        let keep = vec![root.join("a/index.html")];
        let removed = Prune::new(
            &root,
            &root.join("assets"),
            &root.join(".cache"),
            &["themes/**".to_owned()],
        )
        .unwrap()
        .run(&keep)
        .unwrap();

        assert_eq!(removed, 1, "only the orphan outside the glob");
        assert!(root.join("themes/spleen/index.html").exists());
        assert!(root.join("themes").exists());
        assert!(!root.join("stale.html").exists());
    }

    #[test]
    fn a_keep_glob_matches_the_path_relative_to_dist() {
        let (_g, root) = dist(&["paper.pdf", "deep/other.pdf", "page.html"]);
        let removed = Prune::new(
            &root,
            &root.join("assets"),
            &root.join(".cache"),
            &["*.pdf".to_owned()],
        )
        .unwrap()
        .run(&[root.join("page.html")])
        .unwrap();

        assert_eq!(removed, 1);
        assert!(root.join("paper.pdf").exists());
        assert!(!root.join("deep/other.pdf").exists());
    }

    #[test]
    fn an_invalid_keep_glob_is_an_error() {
        let (_g, root) = dist(&["a.html"]);
        let built = Prune::new(
            &root,
            &root.join("assets"),
            &root.join(".cache"),
            &["<//>".to_owned()],
        );
        assert!(built.is_err());
    }

    #[test]
    fn an_empty_keep_set_clears_everything_but_protected() {
        let (_g, root) = dist(&["x.html", "d/y.html", "assets/a.js"]);
        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"), &[])
            .unwrap()
            .run(&[])
            .unwrap();
        assert_eq!(removed, 2);
        assert!(root.join("assets/a.js").exists());
        assert!(!root.join("x.html").exists());
        assert!(!root.join("d").exists());
    }
}
