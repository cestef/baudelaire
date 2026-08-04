//! Walking the content tree into collections.
//!
//! Every `.typ` file under `content/` is assigned to exactly one [`Collection`]:
//! a collection configuring a `glob` claims what it matches, and whatever is
//! left falls back to convention (its top directory, or [`ROOT`] for a file
//! sitting directly under `content/`). Assignment is pure bookkeeping, so the
//! expensive part (loading and evaluating each page) runs in parallel.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use wax::Glob;
use wax::prelude::*;

use crate::config::{CollectionConfig, Config, SortKey};
use crate::content::Page;
use crate::content::cache::DiscoveryCache;
use crate::error::{ContentError, Result};
use crate::world::Project;

/// Special collection id for root-level pages (directly under `content/`).
///
/// A real id, not an internal marker: it is what a `content { collections {
/// _root { .. } } }` block configures, and how a site (or a theme) binds a
/// layout to the pages no other collection claims.
pub const ROOT: &str = "_root";

/// A collection of pages.
#[derive(Debug, Clone)]
pub struct Collection {
    pub id: String,
    pub config: CollectionConfig,
    pub pages: Vec<Page>,
}

impl Collection {
    /// Build a collection for `id`, applying its config override (or convention
    /// default) and sorting its pages accordingly.
    fn new(id: String, pages: Vec<Page>, config: &Config) -> Self {
        let cfg = config.collection(&id).cloned().unwrap_or_default();
        Self {
            id,
            config: cfg,
            pages,
        }
        .sorted()
    }

    /// Sort by the collection's key, breaking ties on source path so that pages
    /// sharing a key (or lacking one entirely) keep a stable order across
    /// machines rather than inheriting directory order.
    fn sorted(mut self) -> Self {
        self.pages.sort_by(|a, b| {
            match self.config.sort {
                SortKey::Order => a.frontmatter.order.cmp(&b.frontmatter.order),
                SortKey::Date => a.frontmatter.date.cmp(&b.frontmatter.date),
                SortKey::Title => a.frontmatter.title.cmp(&b.frontmatter.title),
            }
            .then_with(|| a.source.cmp(&b.source))
        });
        if self.config.reverse {
            self.pages.reverse();
        }
        self
    }
}

/// Discover all collections and pages under `config.paths.content`.
///
/// A collection whose config carries a `glob` claims every content file that
/// pattern matches, wherever it lives. Files no glob claims fall back to
/// convention: one in a subdirectory joins a collection named after that top
/// directory; one directly under `content/` joins `_root` (mapped to `/`).
pub fn discover(config: &Config, project: &Project) -> Result<Vec<Collection>> {
    if !config.paths.content.exists() {
        return Ok(Vec::new());
    }
    let cache = DiscoveryCache::load(config);
    let collections = Discovery::new(config, project).run(&cache)?;
    cache.save()?;
    Ok(collections)
}

/// Assigns discovered content files to collections, glob-configured
/// collections first, then convention for whatever remains.
struct Discovery<'a> {
    config: &'a Config,
    project: &'a Project,
    /// Every content file, paired with whether a collection has claimed it.
    files: Vec<(PathBuf, bool)>,
}

impl<'a> Discovery<'a> {
    fn new(config: &'a Config, project: &'a Project) -> Self {
        Self {
            config,
            project,
            files: Vec::new(),
        }
    }

    fn run(mut self, cache: &DiscoveryCache) -> Result<Vec<Collection>> {
        self.files = Self::gather(&self.config.paths.content)?
            .into_iter()
            .map(|path| (path, false))
            .collect();
        // resolve owners first (cheap, serial), then load + evaluate frontmatter in
        // parallel (the expensive part). `Page::load` records its collection, so the
        // flat result regroups losslessly (rayon preserves input order).
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

    /// Every `.typ` file under `dir`, recursively, skipping dotfiles and
    /// dot-directories.
    fn gather(dir: &Path) -> Result<Vec<PathBuf>> {
        let hidden = |path: &Path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        };
        Ok(crate::fs::Walk::new(dir)
            .skipping(hidden)
            .files()?
            .into_iter()
            .filter(|path| !hidden(path) && path.extension().is_some_and(|e| e == "typ"))
            .collect())
    }

    /// Resolve each content file to its owning collection as `(id, path)` pairs,
    /// in the same order pages are grouped: glob-configured collections first
    /// (config order), then convention for whatever remains. Pure bookkeeping:
    /// no file is read here, so the expensive load can run in parallel.
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
