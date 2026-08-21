use std::path::{Component, Path, PathBuf};

use crate::config::{Config, Permalink};
use crate::content::cache::DiscoveryCache;
use crate::content::discovery::ROOT;
use crate::content::stem::Stem;
use crate::content::{Frontmatter, Slug, Strings};
use crate::error::{ContentError, Result};
use crate::world::Project;

/// How a page's frontmatter reaches its layout template.
#[derive(Debug, Clone)]
pub enum Data {
    /// A real file exporting `#let frontmatter = (..)`: the layout wrapper
    /// imports the export and `#include`s the file.
    Export,
    /// A real file with no export: the wrapper passes an empty dict and
    /// `#include`s the file.
    Empty,
    /// A markdown file: the wrapper inlines the dict lowered from its KDL
    /// frontmatter and the Typst its body lowered to.
    #[cfg(feature = "markdown")]
    Lowered {
        dict: String,
        /// Where the lowered body came from in the file the author wrote, so
        /// diagnostics name the `.md` line rather than generated source.
        sourcemap: std::sync::Arc<crate::content::SourceMap>,
        /// How long the page takes to read, measured on the markdown its author
        /// wrote, because every line of the lowered body opens with `#` and
        /// reads as code.
        reading: crate::engine::text::Reading,
    },
    /// A generated listing with no file: the wrapper inlines `dict` (built by
    /// [`crate::codegen::Value`]) together with the generated body.
    Generated {
        dict: String,
        /// The permalinks the listing lists, which the rendered markup cannot
        /// be asked for because a listing's own template owns those links.
        lists: Vec<String>,
    },
}

impl Data {
    /// Which of the two file-backed shapes a typst page has, by whether its
    /// module exported a `frontmatter`.
    pub(crate) fn of(export: bool) -> Self {
        if export { Self::Export } else { Self::Empty }
    }
}

/// A link to a neighbouring page, exposed to templates as
/// `page.nav.prev`/`page.nav.next`.
#[derive(Hash, Debug, Clone, Default)]
pub struct Sibling {
    pub url: String,
    pub title: String,
}

/// The previous and next pages within a page's collection, in the collection's
/// sort order. Empty for pages with no neighbour and for generated listings.
#[derive(Hash, Debug, Clone, Default)]
pub struct Siblings {
    pub prev: Option<Sibling>,
    pub next: Option<Sibling>,
}

/// One language edition of a page, for a language switcher
/// (`page.translations`) and `hreflang` alternates. A page's own edition is
/// included.
#[derive(Hash, Debug, Clone)]
pub struct Translation {
    pub lang: String,
    pub url: String,
    pub title: String,
}

/// Why a page discovery found is not in the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Withheld {
    /// `draft: true`, and `content { drafts { build } }` is off.
    Draft,
    /// A `date` in the future, and `content { future }` is off.
    Future,
    /// An `expiry` that has passed; no flag brings this one back.
    Expired,
}

/// Stable identifier for a page within the site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PageId(pub String);

impl PageId {
    pub fn new(collection: &str, slug: &str) -> Self {
        Self(format!("{collection}/{slug}"))
    }

    /// The slug half, which is the page's own name within its collection.
    pub fn slug(&self) -> &str {
        self.0
            .rsplit_once('/')
            .map_or(self.0.as_str(), |(_, slug)| slug)
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
    pub data: Data,
    pub collection: String,
    pub permalink: String,
    pub output: PathBuf,
    /// Resolved layout template file (frontmatter, else collection default).
    pub template: Option<String>,
    /// This page's language code (default `lang` on a single-language site).
    pub lang: String,
}

impl Page {
    /// Whether somebody wrote this page, as opposed to the plan having produced
    /// it.
    pub fn authored(&self) -> bool {
        !matches!(self.data, Data::Generated { .. })
    }

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
        let (mut frontmatter, data, body) = cache.load_page(collection, path, config, project)?;
        if frontmatter.source.is_some() && !Config::has_ext(path, Config::MARKDOWN) {
            let source = project.source(path)?;
            let origin = crate::content::Origin::new(&source, path, collection);
            return Err(ContentError::source_on_typst(
                path,
                source.text(),
                origin.entry(Frontmatter::SOURCE),
            )
            .into());
        }
        if path.file_stem().and_then(|s| s.to_str()).is_none() {
            return Err(ContentError::non_utf8_source(path).into());
        }
        let stem = Stem::of(path, config);
        frontmatter.draft |= stem.is_draft();
        let lang = Self::lang(&frontmatter, &stem, path, config)?;
        let raw = frontmatter
            .slug
            .clone()
            .unwrap_or_else(|| Self::bundle_slug(path, collection, &stem, config));
        let slug = Slug::require(&raw)?.into_string();
        let permalink = Self::permalink(collection, &frontmatter, &slug, &lang, path, config);
        let template = config.template_for(collection, frontmatter.template.clone());
        Ok(Self::assemble(
            PageId::new(collection, &slug),
            path.to_owned(),
            frontmatter,
            body,
            data,
            collection.to_owned(),
            &permalink,
            template,
            lang,
            config,
        ))
    }

    /// A page's language: explicit frontmatter `lang`, else the filename
    /// suffix, else the site default. An undeclared language is an error
    /// whichever way it was written.
    fn lang(fm: &Frontmatter, stem: &Stem, path: &Path, config: &Config) -> Result<String> {
        let unknown =
            |code: &str| Err(ContentError::unknown_language(path, code, &config.langs()).into());
        match &fm.lang {
            Some(lang) if !config.knows(lang) => unknown(lang),
            Some(lang) => Ok(lang.clone()),
            None => stem.undeclared(config).map_or_else(
                || Ok(stem.lang().unwrap_or(&config.lang).to_owned()),
                unknown,
            ),
        }
    }

    /// Assemble a page from its resolved parts, deriving the output path from
    /// the permalink. The single `Page { .. }` constructor, shared by authored
    /// pages and generated listings.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assemble(
        id: PageId,
        source: PathBuf,
        frontmatter: Frontmatter,
        body: String,
        data: Data,
        collection: String,
        permalink: &str,
        template: Option<String>,
        lang: String,
        config: &Config,
    ) -> Self {
        let permalink = config.links.style.url(permalink);
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
        }
    }

    pub(super) fn sibling(&self) -> Sibling {
        Sibling {
            url: self.permalink.clone(),
            title: self.title().to_owned(),
        }
    }

    /// The default slug for a page: its parent directory's name when the file
    /// is a bundle index in a real collection, else the file stem.
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

    /// The chain of section names this page nests under:
    /// `content/guide/cli.typ` yields `[guide]`. A bundle index owns its final
    /// directory as its slug, so that directory is dropped.
    pub(crate) fn section_path(&self, config: &Config) -> Vec<String> {
        Self::nesting(&self.source, config)
    }

    /// The same chain, computed from a source path alone, before there is a
    /// [`Page`] to ask.
    pub(crate) fn nesting(source: &Path, config: &Config) -> Vec<String> {
        let rel = source.strip_prefix(&config.paths.content).unwrap_or(source);
        let mut dirs: Vec<String> = rel
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .filter_map(|c| match c {
                Component::Normal(name) => name.to_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        if Stem::of(source, config).is_index(config) {
            dirs.pop();
        }
        dirs
    }

    /// Display title: frontmatter `title`, else the page id.
    pub fn title(&self) -> &str {
        self.frontmatter.title.as_deref().unwrap_or(&self.id.0)
    }

    /// This page's taxonomies as the `(name: (term, ..))` value templates get
    /// as `page.taxonomies`.
    pub fn taxonomies(&self) -> crate::codegen::Value {
        use crate::codegen::Value;
        Value::dict(
            self.frontmatter
                .taxonomies
                .iter()
                .map(|(name, terms)| (name.clone(), Value::array(terms.iter().map(Value::str)))),
        )
    }

    /// Two pages in a collection's or a taxonomy's declared order. Ties break
    /// on source path, so pages sharing a key keep a stable order across
    /// machines.
    pub fn compare(sort: crate::config::SortKey, a: &Self, b: &Self) -> std::cmp::Ordering {
        use crate::config::SortKey;
        match sort {
            SortKey::Order => a.frontmatter.order.cmp(&b.frontmatter.order),
            SortKey::Date => a.frontmatter.date.cmp(&b.frontmatter.date),
            SortKey::Title => a.frontmatter.title.cmp(&b.frontmatter.title),
        }
        .then_with(|| a.source.cmp(&b.source))
    }

    /// Whether this page gets a generated social card, the one answer the
    /// renderer, the `og:image` tag and the prune all read.
    pub fn wants_card(&self, config: &crate::config::Config) -> bool {
        config.artifacts.cards.active()
            && self.frontmatter.image.is_none()
            && !self.frontmatter.excludes(crate::content::Generated::Card)
            && !matches!(self.data, Data::Generated { .. })
    }

    /// Whether this page gets a PDF beside its HTML, the one answer the
    /// exporter, the `<link rel="alternate">` and the prune all read.
    pub fn wants_pdf(&self, config: &crate::config::Config) -> bool {
        config.artifacts.pdf.pages.active()
            && !self.frontmatter.excludes(crate::content::Generated::Pdf)
            && !matches!(self.data, Data::Generated { .. })
    }

    /// The most recent dated pages of one language, newest first, capped at
    /// `limit`. `within` narrows the selection to a single collection.
    pub fn recent<'a>(
        pages: &'a [Self],
        config: &Config,
        lang: &str,
        limit: usize,
        within: Option<&str>,
    ) -> Vec<&'a Self> {
        let candidates = pages.iter().filter(|p| {
            !matches!(p.data, Data::Generated { .. })
                && p.lang == lang
                && p.listed(config)
                && within.is_none_or(|id| p.collection == id)
        });
        Self::newest(candidates, limit)
    }

    /// The collection this page belongs to, as the collection is named in the
    /// config, with the language scope a generated listing carries (`fr/tags`)
    /// stripped back off.
    pub fn section(&self) -> &str {
        self.collection
            .strip_prefix(&format!("{}/", self.lang))
            .unwrap_or(&self.collection)
    }

    /// The newest `limit` dated pages among `pages`, newest first; undated ones
    /// are dropped, and so is a page that excluded itself from the feeds.
    pub fn newest<'a>(pages: impl IntoIterator<Item = &'a Self>, limit: usize) -> Vec<&'a Self> {
        let mut dated: Vec<&Self> = pages
            .into_iter()
            .filter(|p| {
                p.frontmatter.date.is_some()
                    && !p.frontmatter.excludes(crate::content::Generated::Feed)
            })
            .collect();
        dated.sort_by_key(|p| std::cmp::Reverse(p.frontmatter.date));
        dated.truncate(limit);
        dated
    }

    /// Pages bucketed by language, one group per language in first-seen order,
    /// preserving each language's relative order.
    pub fn groups<'a>(pages: &[&'a Self]) -> Vec<Vec<&'a Self>> {
        let mut groups: Vec<(&str, Vec<&Self>)> = Vec::new();
        for &page in pages {
            match groups.iter_mut().find(|(lang, _)| *lang == page.lang) {
                Some((_, group)) => group.push(page),
                None => groups.push((&page.lang, vec![page])),
            }
        }
        groups.into_iter().map(|(_, group)| group).collect()
    }

    /// The key pairing this page with its editions in other languages: a
    /// frontmatter `translation` outright, else its [`PageId`] with the
    /// language scope stripped off.
    pub(super) fn identity(&self) -> String {
        if let Some(key) = &self.frontmatter.translation {
            return key.clone();
        }
        self.id
            .0
            .strip_prefix(&format!("{}/", self.lang))
            .unwrap_or(&self.id.0)
            .to_owned()
    }

    /// The site's authored pages as catalogue rows, keyed by language code and
    /// in the site's own page order. Every built language is a key, so a
    /// template asking for one with no pages reads an empty array.
    pub fn catalogue(
        pages: &[Self],
        config: &Config,
    ) -> std::collections::BTreeMap<String, Vec<crate::codegen::Value>> {
        let mut out: std::collections::BTreeMap<String, Vec<crate::codegen::Value>> =
            std::collections::BTreeMap::new();
        for lang in config.langs() {
            out.entry(lang.to_owned()).or_default();
        }
        for page in pages
            .iter()
            .filter(|p| !matches!(p.data, Data::Generated { .. }) && p.listed(config))
        {
            out.entry(page.lang.clone())
                .or_default()
                .push(page.entry(config));
        }
        out
    }

    /// This page as one catalogue row, the value `@baudelaire/pages` and
    /// `baudelaire:pages` are arrays of, in the same shape a generated
    /// listing's entries take.
    pub fn entry(&self, config: &Config) -> crate::codegen::Value {
        crate::content::listing::Item::of(self, &Strings::new(config, &self.lang)).value()
    }

    /// Whether this page appears in the site's own navigation and indexes. The
    /// not-found page is the one exclusion, and builds either way.
    pub fn listed(&self, config: &Config) -> bool {
        config.not_found(&self.permalink).is_none()
    }

    /// Whether this page builds under the current draft/future config.
    pub fn eligible(&self, config: &Config) -> bool {
        self.withheld(config.content.drafts.build, config.content.future)
            .is_none()
    }

    pub fn skipped(&self, drafts: bool, future: bool) -> bool {
        self.withheld(drafts, future).is_some()
    }

    /// Why this page is not published, or `None` when it is.
    pub fn withheld(&self, drafts: bool, future: bool) -> Option<Withheld> {
        match self {
            _ if self.frontmatter.draft && !drafts => Some(Withheld::Draft),
            _ if self.is_future() && !future => Some(Withheld::Future),
            _ if self.is_expired() => Some(Withheld::Expired),
            _ => None,
        }
    }

    fn is_future(&self) -> bool {
        self.frontmatter
            .date
            .is_some_and(|d| d > time::OffsetDateTime::now_utc().date())
    }

    /// Whether this page's `expiry` has passed; it names the last day the page
    /// is published, so the exclusion starts the day after.
    fn is_expired(&self) -> bool {
        self.frontmatter
            .expiry
            .is_some_and(|d| d < time::OffsetDateTime::now_utc().date())
    }

    /// The permalink a page will resolve to for a given collection, or a root
    /// page when `None`. `source` is where the file will live, which `{path}`
    /// reads.
    pub(crate) fn permalink_of(
        collection: Option<&str>,
        fm: &Frontmatter,
        slug: &str,
        source: &Path,
        config: &Config,
    ) -> String {
        Self::permalink(
            collection.unwrap_or(ROOT),
            fm,
            slug,
            &config.lang,
            source,
            config,
        )
    }

    /// The stem that names a page for its container rather than for itself: a
    /// bundle index, and at the content root the site's home page.
    fn index(config: &Config) -> &str {
        config.index()
    }

    fn permalink(
        collection: &str,
        fm: &Frontmatter,
        slug: &str,
        lang: &str,
        source: &Path,
        config: &Config,
    ) -> String {
        if let Some(named) = fm.path.as_deref() {
            let segments: Vec<&str> = named.split('/').filter(|s| !s.is_empty()).collect();
            let url = if Config::names_a_file(named) {
                format!("/{}", segments.join("/"))
            } else {
                Permalink::join(&segments)
            };
            return config.localize(lang, &url);
        }
        let path = if collection == ROOT {
            let segments: &[&str] = if slug == Self::index(config) {
                &[]
            } else {
                &[slug]
            };
            Permalink::join(segments)
        } else {
            let template = config
                .collection(collection)
                .and_then(|c| c.permalink.as_deref());
            let nesting = Self::nesting(source, config);
            Permalink::of(template).render(&fm.permalink(collection, slug, nesting))
        };
        config.localize(lang, &path)
    }
}

#[cfg(test)]
mod tests {
    use super::{Page, Path};
    use crate::config::Config;
    use crate::content::Frontmatter;

    /// The root index page maps onto `/` under whatever name `content { index
    /// }` gives it.
    #[test]
    fn the_configured_root_index_maps_to_the_site_root() {
        let fm = Frontmatter::default();
        let renamed = Config::parse("content {\n  index \"_index\"\n}").expect("config");
        assert_eq!(
            Page::permalink_of(None, &fm, "_index", Path::new("content/x.typ"), &renamed),
            "/"
        );
        assert_eq!(
            Page::permalink_of(None, &fm, "index", Path::new("content/x.typ"), &renamed),
            "/index/"
        );

        let default = Config::parse("").expect("config");
        let mut named = Frontmatter {
            path: Some("about-us".into()),
            ..Frontmatter::default()
        };
        assert_eq!(
            Page::permalink_of(
                None,
                &named,
                "ignored",
                Path::new("content/x.typ"),
                &default
            ),
            "/about-us/"
        );
        named.path = Some("/about-us/".into());
        assert_eq!(
            Page::permalink_of(
                None,
                &named,
                "ignored",
                Path::new("content/x.typ"),
                &default
            ),
            "/about-us/"
        );
        named.path = Some("/2019/post.html".into());
        assert_eq!(
            Page::permalink_of(
                None,
                &named,
                "ignored",
                Path::new("content/x.typ"),
                &default
            ),
            "/2019/post.html"
        );

        assert_eq!(
            Page::permalink_of(None, &fm, "index", Path::new("content/x.typ"), &default),
            "/"
        );
        assert_eq!(
            Page::permalink_of(None, &fm, "about", Path::new("content/x.typ"), &default),
            "/about/"
        );
    }
}
