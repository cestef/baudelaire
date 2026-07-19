use std::path::{Component, Path, PathBuf};

use rayon::prelude::*;
use wax::Glob;
use wax::prelude::*;

use crate::config::{CollectionConfig, Config, SortKey};
use crate::content::cache::DiscoveryCache;
use crate::content::{Frontmatter, Permalink, PermalinkCtx, Slug};
use crate::error::{ContentError, Result};
use crate::world::Project;

/// How a page's frontmatter reaches its layout template, the single encoding
/// of where a page's data (and body) live.
#[derive(Debug, Clone)]
pub enum Data {
    /// A real file exporting `#let frontmatter = (..)`: the layout wrapper
    /// imports the export and `#include`s the file.
    Export,
    /// A real file with no export: the wrapper passes an empty dict and
    /// `#include`s the file.
    Empty,
    /// A generated listing with no file: the wrapper inlines this dict literal
    /// (built by [`crate::codegen::Value`]) together with the generated body.
    Generated(String),
}

/// A link to a neighbouring page: its URL and display title. Exposed to
/// templates as `page.nav.prev`/`page.nav.next` for prev/next navigation.
#[derive(Debug, Clone, Default)]
pub struct Sibling {
    pub url: String,
    pub title: String,
}

/// The previous and next pages within a page's collection, in the collection's
/// sort order, the "older/newer post" links of a blog. Empty for pages with no
/// neighbour and for generated listings.
#[derive(Debug, Clone, Default)]
pub struct Siblings {
    pub prev: Option<Sibling>,
    pub next: Option<Sibling>,
}

/// One language edition of a page: its code, URL, and title, for a language
/// switcher (`page.translations`) and `hreflang` alternates. A page's own
/// edition is included, so a template can render the full set and mark the
/// current one by `page.lang`.
#[derive(Debug, Clone)]
pub struct Translation {
    pub lang: String,
    pub url: String,
    pub title: String,
}

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
    /// How this page's frontmatter reaches a layout template.
    pub data: Data,
    pub collection: String,
    pub permalink: String,
    pub output: PathBuf,
    /// Resolved layout template file (frontmatter, else collection default).
    pub template: Option<String>,
    /// This page's language code (default `lang` on a single-language site).
    pub lang: String,
    /// Prev/next pages within this page's collection, assigned by
    /// [`crate::content::plan`]. Empty until then, and for generated listings.
    pub siblings: Siblings,
    /// This page's editions in every language (including its own), assigned by
    /// [`crate::content::plan`]. Empty on a single-language site.
    pub translations: Vec<Translation>,
}

impl Page {
    /// Load a single `.typ` file into a [`Page`]: evaluate it as a typst
    /// module (the compiler's own memoized evaluation) and read its
    /// `frontmatter` export.
    pub fn load(
        collection: &str,
        path: &std::path::Path,
        config: &Config,
        project: &Project,
        cache: &DiscoveryCache,
    ) -> Result<Self> {
        // Loading a page evaluates its typst module to read frontmatter; the
        // cache skips both the parse and the evaluation for a page whose source
        // and dependencies are unchanged, returning the body straight from disk.
        let (mut frontmatter, export, body) = cache.load_page(path, config, project)?;
        let data = if export { Data::Export } else { Data::Empty };
        let stem = Stem::of(path, config);
        // A `draft_suffix` in the file stem (e.g. `post.draft.typ`) marks a draft.
        frontmatter.draft |= stem.is_draft();
        let lang = Self::lang(&frontmatter, &stem, path, config)?;
        // explicit frontmatter slug, else the file stem, except a bundle index
        // (`posts/hello/index.typ`) takes its parent dir name, so the directory is
        // one page with colocated resources. reject a name yielding nothing URL-safe.
        let raw = frontmatter
            .slug
            .clone()
            .unwrap_or_else(|| Self::bundle_slug(path, collection, &stem, config));
        let slug = Slug::require(&raw)?.into_string();
        let permalink = Self::permalink(collection, &frontmatter, &slug, &lang, config);
        let template = frontmatter.template.clone().or_else(|| {
            config
                .collection(collection)
                .and_then(|c| c.template.clone())
        });
        Ok(Self::assemble(
            PageId::new(collection, &slug),
            path.to_owned(),
            frontmatter,
            body,
            data,
            collection.to_owned(),
            permalink,
            template,
            lang,
            config,
        ))
    }

    /// A page's language: explicit frontmatter `lang`, else the filename suffix,
    /// else the site default. The single resolution rule; an explicit language
    /// the site does not declare is an error (a suffix is only recognized when
    /// declared, so it can't reach here unknown).
    fn lang(fm: &Frontmatter, stem: &Stem, path: &Path, config: &Config) -> Result<String> {
        match &fm.lang {
            Some(lang) if !config.knows(lang) => {
                Err(ContentError::unknown_language(path, lang, &config.langs()).into())
            }
            Some(lang) => Ok(lang.clone()),
            None => Ok(stem.lang().unwrap_or(&config.lang).to_owned()),
        }
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
        data: Data,
        collection: String,
        permalink: String,
        template: Option<String>,
        lang: String,
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
            lang,
            siblings: Siblings::default(),
            translations: Vec::new(),
        }
    }

    /// This page as a neighbour link, its URL and display title, for a
    /// sibling's prev/next navigation.
    pub(super) fn sibling(&self) -> Sibling {
        Sibling {
            url: self.permalink.clone(),
            title: self.title().to_owned(),
        }
    }

    /// The default slug for a page: its parent directory's name when the file is
    /// a bundle index (stem equals `config.index`) in a real collection, else
    /// the file stem. The root `index.typ` keeps its stem, so it still maps to
    /// `/` rather than to the content directory's name.
    fn bundle_slug(path: &Path, collection: &str, stem: &Stem, config: &Config) -> String {
        let dir = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        match (stem.is_index(config) && collection != ROOT, dir) {
            (true, Some(dir)) => dir.to_owned(),
            _ => stem.slug().to_owned(),
        }
    }

    /// The chain of section names this page nests under, from its location in
    /// the content tree, the basis for a nested nav. `content/guide/cli.typ`
    /// yields `[guide]`; `content/guide/advanced/deep.typ` yields
    /// `[guide, advanced]`. A bundle index (`posts/hello/index.typ`) owns its
    /// final directory as its slug, so that directory is dropped and the page
    /// nests under its parent (`[posts]`).
    pub(crate) fn section_path(&self, config: &Config) -> Vec<String> {
        let rel = self
            .source
            .strip_prefix(&config.content)
            .unwrap_or(&self.source);
        let mut dirs: Vec<String> = rel
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .filter_map(|c| match c {
                Component::Normal(name) => name.to_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        if Stem::of(&self.source, config).is_index(config) {
            dirs.pop();
        }
        dirs
    }

    /// Display title: frontmatter `title`, else the page id. The single
    /// title-fallback rule for every listing and index.
    pub fn title(&self) -> &str {
        self.frontmatter.title.as_deref().unwrap_or(&self.id.0)
    }

    /// This page's taxonomies as the `(name: (term, ..))` value templates get as
    /// `page.taxonomies`, the single serialization shared with the
    /// `baudelaire:pages` module, so their shapes can't drift.
    pub fn taxonomies(&self) -> crate::codegen::Value {
        use crate::codegen::Value;
        Value::dict(
            self.frontmatter
                .taxonomies
                .iter()
                .map(|(name, terms)| (name.clone(), Value::array(terms.iter().map(Value::str)))),
        )
    }

    /// The most recent dated pages of one language, newest first, capped at
    /// `limit`: authored content carrying a date. The single "recent posts"
    /// selection shared by the syndication feeds and the `baudelaire:feed`
    /// module, so a language's feed lists only its own posts.
    pub fn recent<'a>(pages: &'a [Page], lang: &str, limit: usize) -> Vec<&'a Page> {
        let mut dated: Vec<&Page> = pages
            .iter()
            .filter(|p| {
                !matches!(p.data, Data::Generated(_))
                    && p.frontmatter.date.is_some()
                    && p.lang == lang
            })
            .collect();
        dated.sort_by_key(|p| std::cmp::Reverse(p.frontmatter.date));
        dated.truncate(limit);
        dated
    }

    /// Pages bucketed by language, preserving each language's relative order:
    /// one group per language in first-seen order, a single group for a
    /// single-language site. The single language-partition rule, shared wherever
    /// per-language ordering matters (siblings, listings).
    pub fn groups<'a>(pages: &[&'a Page]) -> Vec<Vec<&'a Page>> {
        let mut groups: Vec<(&str, Vec<&Page>)> = Vec::new();
        for &page in pages {
            match groups.iter_mut().find(|(lang, _)| *lang == page.lang) {
                Some((_, group)) => group.push(page),
                None => groups.push((&page.lang, vec![page])),
            }
        }
        groups.into_iter().map(|(_, group)| group).collect()
    }

    /// Fill each content page's `translations` with the editions of the same
    /// logical page in other languages. A page and its translations share a
    /// [`PageId`] (`collection/slug`), so that is the grouping key; only sets
    /// spanning more than one language are recorded, and generated listings are
    /// left out. Editions are ordered by the site's language order (default
    /// first) for a stable switcher.
    pub(super) fn relate(pages: &mut [Page], config: &Config) {
        use std::collections::BTreeMap;
        let mut editions: BTreeMap<&str, Vec<Translation>> = BTreeMap::new();
        for page in pages.iter() {
            if matches!(page.data, Data::Generated(_)) {
                continue;
            }
            editions.entry(&page.id.0).or_default().push(Translation {
                lang: page.lang.clone(),
                url: page.permalink.clone(),
                title: page.title().to_owned(),
            });
        }
        let order = config.langs();
        for set in editions.values_mut() {
            set.sort_by_key(|t| order.iter().position(|l| *l == t.lang));
        }
        let editions: BTreeMap<String, Vec<Translation>> = editions
            .into_iter()
            .filter(|(_, set)| set.len() > 1)
            .map(|(key, set)| (key.to_owned(), set))
            .collect();
        for page in pages.iter_mut() {
            if let Some(set) = editions.get(&page.id.0) {
                page.translations = set.clone();
            }
        }
    }

    /// Whether this page builds under the current draft/future config, the
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

    /// The permalink a page will resolve to for a given collection (or a root
    /// page when `None`), the single rule shared by discovery and by `new`'s
    /// preview, so a scaffolded page reports exactly the URL the build produces.
    pub(crate) fn permalink_of(
        collection: Option<&str>,
        fm: &Frontmatter,
        slug: &str,
        config: &Config,
    ) -> String {
        // `new`'s preview is always for a default-language page.
        Self::permalink(collection.unwrap_or(ROOT), fm, slug, &config.lang, config)
    }

    fn permalink(
        collection: &str,
        fm: &Frontmatter,
        slug: &str,
        lang: &str,
        config: &Config,
    ) -> String {
        let path = if collection == ROOT {
            // The root collection maps straight onto the site root: `index`
            // becomes `/`, every other page a top-level `/{slug}/`.
            let segments: &[&str] = if slug == "index" { &[] } else { &[slug] };
            Permalink::join(segments)
        } else {
            let template = config
                .collection(collection)
                .and_then(|c| c.permalink.as_deref());
            Permalink::of(template).render(&PermalinkCtx::from_page(collection, fm, slug))
        };
        config.localize(lang, &path)
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

/// The parsed stem of a source path: its language and draft markers peeled off,
/// leaving the slug. `post.fr.typ` carries language `fr`; `post.draft.typ` is a
/// draft; the two stack as `post.draft.fr.typ` (language last). The single place
/// a filename is decoded, shared by slugging and section nesting.
struct Stem<'a> {
    /// File stem with the language suffix removed; the draft suffix, if any,
    /// still trails (stripped by [`Stem::slug`]).
    raw: &'a str,
    suffix: &'a str,
    /// Declared non-default language named by a trailing `.{code}`, if any.
    lang: Option<&'a str>,
}

impl<'a> Stem<'a> {
    fn of(path: &'a Path, config: &'a Config) -> Self {
        let full = path.file_stem().and_then(|s| s.to_str()).unwrap_or("index");
        // Peel a trailing `.{code}` naming a declared, non-default language: the
        // default language uses bare filenames, so `.en` on an en site stays put.
        let (raw, lang) = match full.rsplit_once('.') {
            Some((head, code)) if code != config.lang && config.knows(code) => (head, Some(code)),
            _ => (full, None),
        };
        Self {
            raw,
            suffix: &config.draft.suffix,
            lang,
        }
    }

    fn is_draft(&self) -> bool {
        !self.suffix.is_empty() && self.raw.ends_with(self.suffix)
    }

    /// The declared language named by the filename, if any.
    fn lang(&self) -> Option<&'a str> {
        self.lang
    }

    /// Whether this stem names a bundle index (`config.index`), so the file's
    /// parent directory supplies the slug rather than the file name.
    fn is_index(&self, config: &Config) -> bool {
        config
            .index
            .as_deref()
            .is_some_and(|idx| self.slug() == idx)
    }

    fn slug(&self) -> &'a str {
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
pub fn discover(config: &Config, project: &Project) -> Result<Vec<Collection>> {
    if !config.content.exists() {
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
        self.files = Self::gather(&self.config.content)?
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
    /// (config order), then convention for whatever remains. Pure bookkeeping:
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
