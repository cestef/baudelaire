use std::path::{Path, PathBuf};

use rayon::prelude::*;
use typst::syntax::Source;
use wax::Glob;
use wax::prelude::*;

use crate::config::{CollectionConfig, Config, SortKey};
use crate::content::{Frontmatter, Permalink, PermalinkCtx, Slug};
use crate::error::{ContentError, Result};

/// Stable identifier for a page within the site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PageId(pub String);

impl PageId {
    pub fn new(collection: &str, slug: &str) -> Self {
        Self(format!("{collection}/{slug}"))
    }
}

impl std::fmt::Display for PageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A discovered content page.
#[derive(Debug, Clone)]
pub struct Page {
    pub id: PageId,
    pub source: PathBuf,
    pub frontmatter: Frontmatter,
    pub body: String,
    /// The frontmatter argument dict literal, passed to a layout template.
    pub data: String,
    pub collection: String,
    pub permalink: String,
    pub output: PathBuf,
    /// Resolved layout template file (frontmatter, else collection default).
    pub template: Option<String>,
}

impl Page {
    /// Load and parse a single `.typ` file into a [`Page`].
    pub fn load(collection: &str, path: &std::path::Path, config: &Config) -> Result<Self> {
        let text = crate::fs::read_to_string(path)?;
        let src = Source::detached(&text);
        let (mut frontmatter, body, data) = match Frontmatter::extract(&src, path, config)? {
            Some(e) => (e.frontmatter, e.body, e.data),
            // An empty typst dict is `(:)` — `()` is the empty *array*, which a
            // template's `data.frontmatter.at(..)` cannot index.
            None => (Frontmatter::default(), text, "(:)".to_owned()),
        };
        let stem = Stem::of(path, &config.draft.suffix);
        // A `draft_suffix` in the file stem (e.g. `post.draft.typ`) marks a draft.
        frontmatter.draft |= stem.is_draft();
        // One slug policy: normalize the frontmatter slug or the file stem, and
        // reject a name that yields nothing URL-safe rather than emit `//`.
        let raw = frontmatter.slug.clone().unwrap_or_else(|| stem.slug().to_owned());
        let slug = Slug::require(&raw)?.into_string();
        let permalink = Self::permalink(collection, &frontmatter, &slug, config);
        let template = frontmatter
            .template
            .clone()
            .or_else(|| config.collection(collection).and_then(|c| c.template.clone()));
        Ok(Self::assemble(
            PageId::new(collection, &slug),
            path.to_owned(),
            frontmatter,
            body,
            data,
            collection.to_owned(),
            permalink,
            template,
            config,
        ))
    }

    /// Assemble a page from its resolved parts, deriving the output path from the
    /// permalink. The single `Page { .. }` constructor: real pages ([`Page::load`])
    /// and synthetic listings ([`crate::content::listing::Listing::into_page`])
    /// both build through here, so a new field can never be set in one and
    /// forgotten in the other.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assemble(
        id: PageId,
        source: PathBuf,
        frontmatter: Frontmatter,
        body: String,
        data: String,
        collection: String,
        permalink: String,
        template: Option<String>,
        config: &Config,
    ) -> Self {
        Self {
            output: config.destination(&permalink),
            id,
            source,
            frontmatter,
            body,
            data,
            collection,
            permalink,
            template,
        }
    }

    /// Display title: frontmatter `title`, else the page id. The single
    /// title-fallback rule for every listing and index.
    pub fn title(&self) -> &str {
        self.frontmatter.title.as_deref().unwrap_or(&self.id.0)
    }

    /// Whether this page builds under the current draft/future config — the
    /// one eligibility predicate, shared by the engine and page generators.
    pub fn eligible(&self, config: &Config) -> bool {
        !self.skipped(config.draft.build, config.future)
    }

    /// Whether this page should be skipped given draft/future flags.
    pub fn skipped(&self, drafts: bool, future: bool) -> bool {
        (self.frontmatter.draft && !drafts) || (self.is_future() && !future)
    }

    fn is_future(&self) -> bool {
        self.frontmatter
            .date
            .is_some_and(|d| d > time::OffsetDateTime::now_utc().date())
    }

    fn permalink(collection: &str, fm: &Frontmatter, slug: &str, config: &Config) -> String {
        if collection == ROOT {
            // The root collection maps straight onto the site root: `index`
            // becomes `/`, every other page a top-level `/{slug}/`.
            let segments: &[&str] = if slug == "index" { &[] } else { &[slug] };
            return Permalink::join(segments);
        }
        let template = config
            .collection(collection)
            .and_then(|c| c.permalink.as_deref());
        Permalink::of(template).render(&PermalinkCtx::from_page(collection, fm, slug))
    }

}

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

    fn sorted(mut self) -> Self {
        self.pages.sort_by(|a, b| match self.config.sort {
            SortKey::Order => a.frontmatter.order.cmp(&b.frontmatter.order),
            SortKey::Date => a.frontmatter.date.cmp(&b.frontmatter.date),
            SortKey::Title => a.frontmatter.title.cmp(&b.frontmatter.title),
        });
        if self.config.reverse {
            self.pages.reverse();
        }
        self
    }
}

/// The slug-bearing stem of a source path. A configured `draft_suffix` in the
/// stem (e.g. `post.draft.typ`) both marks the page a draft and is stripped
/// from its slug.
struct Stem<'a> {
    raw: &'a str,
    suffix: &'a str,
}

impl<'a> Stem<'a> {
    fn of(path: &'a Path, suffix: &'a str) -> Self {
        let raw = path.file_stem().and_then(|s| s.to_str()).unwrap_or("index");
        Self { raw, suffix }
    }

    fn is_draft(&self) -> bool {
        !self.suffix.is_empty() && self.raw.ends_with(self.suffix)
    }

    fn slug(&self) -> &str {
        if self.suffix.is_empty() {
            return self.raw;
        }
        self.raw.strip_suffix(self.suffix).unwrap_or(self.raw)
    }
}

/// Special collection id for root-level pages (directly under `content/`).
const ROOT: &str = "_root";

/// Discover all collections and pages under `config.content`.
///
/// A collection whose config carries a `glob` claims every content file that
/// pattern matches, wherever it lives. Files no glob claims fall back to
/// convention: one in a subdirectory joins a collection named after that top
/// directory; one directly under `content/` joins `_root` (mapped to `/`).
pub fn discover(config: &Config) -> Result<Vec<Collection>> {
    if !config.content.exists() {
        return Ok(Vec::new());
    }
    Discovery::new(config).run()
}

/// Assigns discovered content files to collections — glob-configured
/// collections first, then convention for whatever remains.
struct Discovery<'a> {
    config: &'a Config,
    /// Every content file, paired with whether a collection has claimed it.
    files: Vec<(PathBuf, bool)>,
}

impl<'a> Discovery<'a> {
    fn new(config: &'a Config) -> Self {
        Self {
            config,
            files: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Vec<Collection>> {
        self.files = Self::gather(&self.config.content)?
            .into_iter()
            .map(|path| (path, false))
            .collect();
        // Resolve which collection owns each file first (cheap, serial), then
        // load + evaluate every page's frontmatter in parallel — the expensive
        // part. `Page::load` records its collection, so the flat parallel result
        // regroups losslessly (rayon preserves input order).
        let assignments = self.assign()?;
        let pages: Vec<Page> = assignments
            .par_iter()
            .map(|(id, path)| Page::load(id, path, self.config))
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
        let mut out = Vec::new();
        for path in crate::fs::read_dir(dir)? {
            let hidden = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'));
            if hidden {
                continue;
            }
            if path.is_dir() {
                out.extend(Self::gather(&path)?);
            } else if path.extension().is_some_and(|e| e == "typ") {
                out.push(path);
            }
        }
        Ok(out)
    }

    /// Resolve each content file to its owning collection as `(id, path)` pairs,
    /// in the same order pages are grouped: glob-configured collections first
    /// (config order), then convention for whatever remains. Pure bookkeeping —
    /// no file is read here, so the expensive load can run in parallel.
    fn assign(&mut self) -> Result<Vec<(String, PathBuf)>> {
        let mut out = Vec::new();
        let globs: Vec<(String, String)> = self
            .config
            .collections
            .iter()
            .filter_map(|(id, cfg)| Some((id.clone(), cfg.glob.clone()?)))
            .collect();
        for (id, glob) in globs {
            let pattern = Glob::new(&glob).map_err(|e| ContentError::bad_glob(&glob, e))?;
            for (path, taken) in &mut self.files {
                let rel = path.strip_prefix(&self.config.content).unwrap_or(path);
                if !*taken && pattern.is_match(rel) {
                    *taken = true;
                    out.push((id.clone(), path.clone()));
                }
            }
        }
        for (path, taken) in &self.files {
            if !taken {
                let rel = path.strip_prefix(&self.config.content).unwrap_or(path);
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
