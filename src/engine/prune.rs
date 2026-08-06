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

use wax::{Glob, Program};

use crate::error::{ContentError, Result};
use crate::fs;

/// Deletes files under `dist` that are not in the produced set.
///
/// Containment is the only thing bounding what this deletes, so it is safe
/// exactly as far as `dist` is a directory of the build's own. That `dist` does
/// not contain the content, asset, static, or template trees is established
/// once, by [`Paths::swallowed`] at engine construction; nothing here re-checks
/// it, and nothing here could.
///
/// [`Paths::swallowed`]: crate::config::Paths::swallowed
pub struct Prune<'a> {
    dist: &'a Path,
    /// `dist` on disk, to test containment against: the one boundary the sweep
    /// may not delete outside of.
    root: PathBuf,
    /// Directory prefixes owned by another stage (the asset pipeline) or outside
    /// the build (the cache): never walked, never pruned.
    protected: Vec<PathBuf>,
    /// `prune { keep }`: globs, relative to `dist`, whose matches survive
    /// whether or not this build produced them.
    ///
    /// Globs rather than prefixes, unlike [`protected`](Self::protected),
    /// because these name what somebody else wrote there and that is as often a
    /// shape (`*.pdf`) as a subtree. They spare files only: a directory holding
    /// one is left standing by the empty-directory sweep below, which is the
    /// same thing that keeps a directory of surviving pages.
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
            if keep.contains(&fs::canonical(file)) || self.spared(file) {
                continue;
            }
            fs::remove_file(file)?;
            removed += 1;
        }
        // `dirs` comes back children-first, so a directory the sweep emptied is
        // dropped after its contents; a still-populated one (kept files, or a
        // skipped subtree) errors and is ignored.
        for dir in &tree.dirs {
            let _ = std::fs::remove_dir(dir);
        }
        Ok(removed)
    }

    /// Whether `prune { keep }` claims this file, matched on its path relative
    /// to `dist`: the spelling the author wrote the glob in, and the only one
    /// that is stable across an absolute `dist`, a relative one, and a
    /// symlinked one.
    ///
    /// A file that somehow walked up outside `dist` is not spared, and does not
    /// need to be: [`owns`](Self::owns) refused to walk there.
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
        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"), &[])
            .unwrap()
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
        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"), &[])
            .unwrap()
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

        let removed = Prune::new(&root, &root.join("assets"), &root.join(".cache"), &[])
            .unwrap()
            .run(&[root.join("a/index.html")])
            .unwrap();

        assert_eq!(removed, 0);
        assert!(outside.join("secret.txt").exists());
    }

    /// `prune { keep }`: what another tool wrote into `dist` survives a build
    /// that knows nothing about it, whatever order the two ran in.
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
        // The directory holding a spared file is left standing with it.
        assert!(root.join("themes").exists());
        assert!(!root.join("stale.html").exists());
    }

    /// A glob names a shape as readily as a subtree, and matches at any depth
    /// only where it says so: `*.pdf` is the top level, `**/*.pdf` is anywhere.
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

    /// An unparseable glob is refused where it is compiled, not silently
    /// ignored: a keep list that quietly matches nothing is a deleted site.
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
