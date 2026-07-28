use std::path::{Component, Path, PathBuf};

use crate::config::{Config, Permalink};
use crate::content::cache::DiscoveryCache;
use crate::content::discovery::ROOT;
use crate::content::stem::Stem;
use crate::content::{Frontmatter, Slug};
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
        // Reject a name that is not text before decoding it. `Stem::of` falls
        // back to `index` for one, which is the *bundle index* name: the file
        // silently took its parent directory's slug and could overwrite the
        // real page there.
        if path.file_stem().and_then(|s| s.to_str()).is_none() {
            return Err(ContentError::non_utf8_source(path).into());
        }
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
    /// else the site default. The single resolution rule.
    ///
    /// An undeclared language is an error whichever way it was written. A
    /// suffix only *resolves* when declared, so `post.fr.typ` on a site without
    /// `fr` used to fall through and publish at `/post.fr/` as a
    /// default-language page, while the very same typo spelled `lang: "fr"`
    /// stopped the build: one mistake, two opposite outcomes.
    fn lang(fm: &Frontmatter, stem: &Stem, path: &Path, config: &Config) -> Result<String> {
        let unknown =
            |code: &str| Err(ContentError::unknown_language(path, code, &config.langs()).into());
        match &fm.lang {
            Some(lang) if !config.knows(lang) => unknown(lang),
            Some(lang) => Ok(lang.clone()),
            None => match stem.undeclared(config) {
                Some(code) => unknown(code),
                None => Ok(stem.lang().unwrap_or(&config.lang).to_owned()),
            },
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
        // Shape the URL for the site's style here, the one funnel every page
        // (authored and generated) passes through, so the permalink and the
        // file it maps to can never disagree.
        let permalink = config.links.style.url(&permalink);
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
    /// a bundle index (stem equals `config.content.index`) in a real collection, else
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
            .strip_prefix(&config.paths.content)
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

    /// Whether this page gets a generated social card.
    ///
    /// Three conditions, in one place because the renderer, the `og:image` tag,
    /// and the prune all have to agree: cards are configured *and* compiled in,
    /// the page named no image of its own (an authored one always wins), and it
    /// is real content rather than a generated listing (nobody shares a tag
    /// index, and one render per term would dominate the build).
    pub fn wants_card(&self, config: &crate::config::Config) -> bool {
        config.generate.cards.active()
            && self.frontmatter.text("image").is_none()
            && !matches!(self.data, Data::Generated(_))
    }

    /// The most recent dated pages of one language, newest first, capped at
    /// `limit`: authored content carrying a date. The single "recent posts"
    /// selection shared by the syndication feeds and the `baudelaire:feed`
    /// module, so a language's feed lists only its own posts.
    pub fn recent<'a>(pages: &'a [Page], lang: &str, limit: usize) -> Vec<&'a Page> {
        let candidates = pages
            .iter()
            .filter(|p| !matches!(p.data, Data::Generated(_)) && p.lang == lang);
        Self::newest(candidates, limit)
    }

    /// The newest `limit` dated pages among `pages`, newest first; undated ones
    /// are dropped. The single ordering rule every feed is built on: the site
    /// feed hands it a language's pages, a term feed hands it that term's
    /// members, and both come out in the same order.
    pub fn newest<'a>(pages: impl IntoIterator<Item = &'a Page>, limit: usize) -> Vec<&'a Page> {
        let mut dated: Vec<&Page> = pages
            .into_iter()
            .filter(|p| p.frontmatter.date.is_some())
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

    /// Fill each page's `translations` with the editions of the same logical
    /// page in other languages, generated listings included. Only sets spanning
    /// more than one language are recorded. Editions are ordered by the site's
    /// language order (default first) for a stable switcher.
    pub(super) fn relate(pages: &mut [Page], config: &Config) {
        use std::collections::BTreeMap;
        let mut editions: BTreeMap<String, Vec<Translation>> = BTreeMap::new();
        for page in pages.iter() {
            editions
                .entry(page.identity())
                .or_default()
                .push(Translation {
                    lang: page.lang.clone(),
                    url: page.permalink.clone(),
                    title: page.title().to_owned(),
                });
        }
        let order = config.langs();
        editions.retain(|_, set| set.len() > 1);
        for set in editions.values_mut() {
            set.sort_by_key(|t| order.iter().position(|l| *l == t.lang));
        }
        for page in pages.iter_mut() {
            if let Some(set) = editions.get(&page.identity()) {
                page.translations = set.clone();
            }
        }
    }

    /// The key pairing this page with its editions in other languages.
    ///
    /// A page's [`PageId`] is `collection/slug`, which already matches across
    /// languages for authored pages. A generated listing's collection is the
    /// *scoped* section (`fr/tags`), so its id differed per language and no
    /// listing ever had a translation: no language switcher, and no `hreflang`
    /// in the sitemap. Stripping the scope back off restores the pairing.
    fn identity(&self) -> String {
        self.id
            .0
            .strip_prefix(&format!("{}/", self.lang))
            .unwrap_or(&self.id.0)
            .to_owned()
    }

    /// Whether this page builds under the current draft/future config, the
    /// one eligibility predicate, shared by the engine and page generators.
    pub fn eligible(&self, config: &Config) -> bool {
        !self.skipped(config.content.draft.build, config.content.future)
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

    /// The stem that names a page for its container rather than for itself: a
    /// bundle index, and at the content root the site's home page. Reads
    /// `content { index }`, so a site spelling it `_index` publishes its root
    /// page at `/` like any other; hardcoding `index` here sent that page to
    /// `/_index/` and left the site without a home.
    ///
    /// The `index` fallback is the one every call site applies when the key is
    /// unset (`Stem::FALLBACK`, `Config::bundle_index`); it belongs on
    /// `ContentConfig` with those, not spelled once more per caller.
    fn index(config: &Config) -> &str {
        config.content.index.as_deref().unwrap_or("index")
    }

    fn permalink(
        collection: &str,
        fm: &Frontmatter,
        slug: &str,
        lang: &str,
        config: &Config,
    ) -> String {
        let path = if collection == ROOT {
            // The root collection maps straight onto the site root: the bundle
            // index becomes `/`, every other page a top-level `/{slug}/`.
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
            Permalink::of(template).render(&fm.permalink(collection, slug))
        };
        config.localize(lang, &path)
    }
}

#[cfg(test)]
mod tests {
    use super::Page;
    use crate::config::Config;
    use crate::content::Frontmatter;

    /// The root index page maps onto `/` under whatever name `content { index }`
    /// gives it. Hardcoding `index` meant a site configured with `_index`
    /// published its root page at `/_index/` and had no home page at all, while
    /// the rest of the code (bundle slugs, section nesting) read the config.
    #[test]
    fn the_configured_root_index_maps_to_the_site_root() {
        let fm = Frontmatter::default();
        let renamed = Config::parse("content {\n  index \"_index\"\n}").expect("config");
        assert_eq!(Page::permalink_of(None, &fm, "_index", &renamed), "/");
        // ...and the default name is then just another top-level page.
        assert_eq!(Page::permalink_of(None, &fm, "index", &renamed), "/index/");

        let default = Config::parse("").expect("config");
        assert_eq!(Page::permalink_of(None, &fm, "index", &default), "/");
        assert_eq!(Page::permalink_of(None, &fm, "about", &default), "/about/");
    }
}
