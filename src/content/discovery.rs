//! Walking the content tree into collections: every content file under
//! `content/` is assigned to exactly one [`Collection`].

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use wax::Glob;
use wax::prelude::*;

use crate::config::{CollectionConfig, Config, Paths};
use crate::content::Page;
use crate::content::cache::DiscoveryCache;
use crate::error::{ContentError, Result};
use crate::world::Project;

/// Collection id for root-level pages (directly under `content/`); a real id a
/// `content { collections { _root { .. } } }` block can configure.
pub const ROOT: &str = "_root";

#[derive(Debug, Clone)]
pub struct Collection {
    pub id: String,
    pub config: CollectionConfig,
    pub pages: Vec<Page>,
}

impl Collection {
    fn new(id: String, pages: Vec<Page>, config: &Config) -> Self {
        let cfg = config.collection(&id).cloned().unwrap_or_default();
        Self {
            id,
            config: cfg,
            pages,
        }
        .sorted()
    }

    fn sorted(mut self) -> Self {
        let sort = self.config.sort;
        self.pages.sort_by(|a, b| Page::compare(sort, a, b));
        if self.config.reverse {
            self.pages.reverse();
        }
        self
    }
}

/// Assigns discovered content files to collections, glob-configured
/// collections first, then convention for whatever remains.
pub struct Discovery<'a> {
    config: &'a Config,
    project: &'a Project,
    /// Every content file, paired with whether a collection has claimed it.
    files: Vec<(PathBuf, bool)>,
}

impl<'a> Discovery<'a> {
    /// Every collection and page under `config.paths.content`.
    ///
    /// A collection whose config carries a `glob` claims every content file
    /// that pattern matches, wherever it lives. Files no glob claims fall back
    /// to convention: one in a subdirectory joins a collection named after that
    /// top directory; one directly under `content/` joins `_root` (mapped to
    /// `/`).
    ///
    /// A missing content directory is an empty site when nothing named one, and
    /// an error the walk reports when something did.
    pub fn all(config: &'a Config, project: &'a Project) -> Result<Vec<Collection>> {
        if !config.paths.content.exists() && !Self::named(config) {
            return Ok(Vec::new());
        }
        let tracked = project.tracked();
        let cache = DiscoveryCache::load(config, project, &tracked);
        let collections = Self::new(config, project).run(&cache)?;
        cache.save()?;
        Ok(collections)
    }

    fn new(config: &'a Config, project: &'a Project) -> Self {
        Self {
            config,
            project,
            files: Vec::new(),
        }
    }

    fn run(mut self, cache: &DiscoveryCache) -> Result<Vec<Collection>> {
        self.files = Self::gather(&self.config.paths.content, &self.config.sources())?
            .into_iter()
            .map(|path| (path, false))
            .collect();
        let assignments = self.assign()?;
        let pages: Vec<Page> = assignments
            .par_iter()
            .map(|(id, path)| Page::load(id, path, self.config, self.project, cache))
            .collect::<Result<Vec<_>>>()?;
        let mut groups: Vec<(String, Vec<Page>)> = Vec::new();
        for page in pages {
            match groups.iter_mut().find(|(id, _)| *id == page.collection) {
                Some((_, list)) => list.push(page),
                None => groups.push((page.collection.clone(), vec![page])),
            }
        }
        Ok(groups
            .into_iter()
            .map(|(id, pages)| Collection::new(id, pages, self.config))
            .collect())
    }

    /// Whether the site named its content directory something other than the
    /// default, which is what makes a missing one an error rather than an empty
    /// site. Compared on the final component alone, because a path resolved
    /// against a project root arrives absolute and would otherwise read as
    /// named on every site there is.
    fn named(config: &Config) -> bool {
        let default = Paths::default().content;
        config.paths.content.file_name() != default.file_name()
    }

    /// Every content file under `dir`, recursively, skipping dotfiles and
    /// dot-directories.
    ///
    /// The walk follows a link out of the tree: a site keeping its pages in a
    /// vault and linking a subtree in is compiling those files, not copying
    /// them, and each is published at the permalink its own frontmatter names.
    fn gather(dir: &Path, sources: &[&str]) -> Result<Vec<PathBuf>> {
        let hidden = |path: &Path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        };
        Ok(crate::fs::Walk::new(dir)
            .following()
            .skipping(hidden)
            .files()?
            .into_iter()
            .filter(|path| !hidden(path) && sources.iter().any(|ext| Config::has_ext(path, ext)))
            .collect())
    }

    /// Resolve each content file to its owning collection as `(id, path)`
    /// pairs, in the same order pages are grouped: glob-configured collections
    /// first (config order), then convention for whatever remains.
    fn assign(&mut self) -> Result<Vec<(String, PathBuf)>> {
        let mut out = Vec::new();
        let globs: Vec<(String, String)> = self
            .config
            .content
            .collections
            .iter()
            .filter_map(|(id, cfg)| Some((id.clone(), cfg.glob.clone()?)))
            .collect();
        for (id, glob) in globs {
            let pattern =
                Glob::new(&glob).map_err(|e| ContentError::bad_glob("collection", &glob, e))?;
            for (path, taken) in &mut self.files {
                let rel = path
                    .strip_prefix(&self.config.paths.content)
                    .unwrap_or(path);
                if !*taken && pattern.is_match(rel) {
                    *taken = true;
                    out.push((id.clone(), path.clone()));
                }
            }
        }
        for (path, taken) in &self.files {
            if !taken {
                let rel = path
                    .strip_prefix(&self.config.paths.content)
                    .unwrap_or(path);
                out.push((Self::convention_id(rel), path.clone()));
            }
        }
        Ok(out)
    }

    /// The convention collection id for a content-relative path: the top
    /// directory, or `_root` for a file directly under `content/`.
    fn convention_id(rel: &Path) -> String {
        let mut components = rel.components();
        match (components.next(), components.next()) {
            (Some(dir), Some(_)) => dir.as_os_str().to_string_lossy().into_owned(),
            _ => ROOT.to_owned(),
        }
    }
}
