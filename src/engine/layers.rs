//! Stacked source directories: a theme's `templates/`, `assets/` and `static/`
//! under the project's own, where the project's file always wins.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::fs;

/// One file resolved through the stack: where it lives, and the relative path
/// it is known by, which is carried because with more than one root there is no
/// single prefix to strip.
#[derive(Debug, Clone)]
pub(super) struct Layered {
    /// Path relative to whichever root this file came from: its identity in the
    /// output, the asset map, and every URL derived from it.
    pub rel: PathBuf,
    /// Where to read it.
    pub path: PathBuf,
}

/// An ordered stack of source directories, later ones overriding earlier.
pub(super) struct Layers(Vec<PathBuf>);

impl Layers {
    /// A theme's directory (when there is one) beneath the project's, so the
    /// project's file at a given relative path always wins.
    pub(super) fn new(theme: Option<PathBuf>, project: &Path) -> Self {
        Self(theme.into_iter().chain([project.to_path_buf()]).collect())
    }

    /// The stack as a search path, the strongest root first, which is the
    /// reverse of the stack's own order: the Sass compiler resolves a `@use` by
    /// searching directories rather than by overwriting.
    #[cfg(feature = "sass")]
    pub(super) fn search(&self) -> Vec<PathBuf> {
        self.0.iter().rev().cloned().collect()
    }

    /// Every file across the stack, keyed by relative path, ordered by that
    /// path so a build is deterministic whatever order the filesystem walks in.
    ///
    /// A missing directory is not an error: both the theme's and the project's
    /// are optional, and a site with neither simply has no files here.
    pub(super) fn files(&self) -> Result<Vec<Layered>> {
        let mut found: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
        for root in &self.0 {
            if !root.exists() {
                continue;
            }
            for path in fs::Walk::new(root).files()? {
                let rel = path
                    .strip_prefix(root)
                    .expect("Walk yields paths under root")
                    .to_path_buf();
                found.insert(rel, path);
            }
        }
        Ok(found
            .into_iter()
            .map(|(rel, path)| Layered { rel, path })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a tree of `(relative path, contents)` under a fresh directory.
    fn tree(dir: &Path, files: &[&str]) {
        for rel in files {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(&path, rel).expect("write");
        }
    }

    #[test]
    fn the_project_overrides_the_theme_file_by_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (theme, project) = (tmp.path().join("theme"), tmp.path().join("project"));
        tree(&theme, &["a.css", "shared.css", "deep/x.css"]);
        tree(&project, &["shared.css", "b.css"]);

        let files = Layers::new(Some(theme.clone()), &project)
            .files()
            .expect("files");
        let by_rel: BTreeMap<_, _> = files
            .iter()
            .map(|f| (f.rel.to_string_lossy().into_owned(), f.path.clone()))
            .collect();

        assert_eq!(by_rel.len(), 4, "{by_rel:?}");
        assert_eq!(by_rel["a.css"], theme.join("a.css"));
        assert_eq!(by_rel["deep/x.css"], theme.join("deep/x.css"));
        assert_eq!(by_rel["shared.css"], project.join("shared.css"));
        assert_eq!(by_rel["b.css"], project.join("b.css"));
    }

    #[test]
    fn a_stack_without_a_theme_is_just_the_project() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project = tmp.path().join("project");
        tree(&project, &["a.css"]);

        let files = Layers::new(None, &project).files().expect("files");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel, Path::new("a.css"));
    }

    #[test]
    fn missing_directories_are_not_errors() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let layers = Layers::new(Some(tmp.path().join("nope")), &tmp.path().join("gone"));
        assert!(layers.files().expect("files").is_empty());
    }
}
