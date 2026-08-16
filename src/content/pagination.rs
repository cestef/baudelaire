//! Collection index pages, optionally paginated: a collection that configures
//! `paginate { }` gets a generated index [`Listing`] at `/{collection}/`, its
//! members chunked across `/{collection}/page/2/`, .. when it names a size.

use crate::config::{Config, Permalink};
use crate::content::generate::{Generate, PlanCtx};
use crate::content::listing::{Item, Listing, Nav, Titlecase};
use crate::content::{Collection, Page, Strings};
use crate::error::Result;

/// A membership chunked into numbered pages, with the URL and slug rules that
/// follow from where the listing sits; the single pagination rule, shared by
/// collection indexes and taxonomy term listings.
pub(crate) struct Paged<'a> {
    /// The segments page 2 and later hang under, unlocalized: `["blog"]` for a
    /// collection, `["tags", "rust"]` for a term.
    root: &'a [&'a str],
    /// Where page 1 sits, unlocalized; `None` puts it at `root`, which is what
    /// a taxonomy term does.
    mount: Option<&'a str>,
    /// Path segment before the number (`/blog/page/2/`); empty drops it.
    prefix: &'a str,
    /// The slug page 1 takes within its section; later pages extend it. Empty
    /// means the section index.
    slug: &'a str,
    /// Members per page; never zero, since `chunks(0)` panics.
    per_page: usize,
    lang: &'a str,
    config: &'a Config,
}

impl<'a> Paged<'a> {
    /// The page-number segment when no `prefix` names one, since a slug has to
    /// stay a valid identifier and cannot be the bare number.
    const WORD: &'static str = "page";

    pub(crate) fn new(
        root: &'a [&'a str],
        mount: Option<&'a str>,
        prefix: &'a str,
        slug: &'a str,
        per_page: usize,
        lang: &'a str,
        config: &'a Config,
    ) -> Self {
        Self {
            root,
            mount,
            prefix,
            slug,
            per_page: per_page.max(1),
            lang,
            config,
        }
    }

    /// The members split across pages; never empty, so a memberless listing
    /// still gets the page 1 its nav links point at.
    pub(crate) fn chunks<'m, T>(&self, members: &'m [T]) -> Vec<&'m [T]> {
        let mut chunks: Vec<&[T]> = members.chunks(self.per_page).collect();
        if chunks.is_empty() {
            chunks.push(&[]);
        }
        chunks
    }

    /// Page 1 sits at the `mount` or the root; later pages under
    /// `{root}/{prefix}/{n}/`, or `{root}/{n}/` when `prefix` is empty. Every
    /// URL is localized to the listing's language.
    pub(crate) fn url(&self, number: usize) -> String {
        let number = number.to_string();
        let raw = match (number.as_str(), self.mount) {
            ("1", Some(mount)) => mount.to_owned(),
            ("1", None) => Permalink::join(self.root),
            _ => {
                let mut segments = self.root.to_vec();
                segments.push(self.prefix);
                segments.push(&number);
                Permalink::join(&segments)
            }
        };
        self.config.localize(self.lang, &raw)
    }

    /// The internal page slug (its id within the section, not a URL).
    pub(crate) fn slug(&self, number: usize) -> String {
        let word = if self.prefix.is_empty() {
            Self::WORD
        } else {
            self.prefix
        };
        match (number, self.slug) {
            (1, "") => Listing::INDEX.to_owned(),
            (1, slug) => slug.to_owned(),
            (n, "") => format!("{word}-{n}"),
            (n, slug) => format!("{slug}-{word}-{n}"),
        }
    }

    pub(crate) fn nav(&self, number: usize, total: usize) -> Nav {
        Nav {
            prev: (number > 1).then(|| self.url(number - 1)),
            next: (number < total).then(|| self.url(number + 1)),
        }
    }

    /// How a page past the first is titled: the listing's own title, then the
    /// localized word for "page" and the number.
    pub(crate) fn title(&self, base: &str, number: usize) -> String {
        match number {
            1 => base.to_owned(),
            n => format!(
                "{base} - {} {n}",
                Strings::new(self.config, self.lang).get("page")
            ),
        }
    }
}

/// Builds collection index pages (paginated when the collection sets a count).
pub struct Pagination;

impl Generate for Pagination {
    /// Generate index pages for every collection that asks for one, over its
    /// build-eligible members, one index per language.
    fn generate(&self, ctx: &PlanCtx) -> Result<Vec<Page>> {
        let mut out = Vec::new();
        for collection in ctx.collections {
            for lang in ctx.config.langs() {
                if let Some(section) = Section::of(collection, ctx.config, lang) {
                    section.build(&mut out);
                }
            }
        }
        Ok(out)
    }
}

/// One collection's build-eligible members in a single language, chunked into
/// pages.
struct Section<'a> {
    id: &'a str,
    template: Option<String>,
    /// Permalink of page 1 ([`crate::config::CollectionConfig::home`]); later
    /// pages hang under it.
    mount: String,
    /// Path segment before a page number (`/{id}/{prefix}/{n}/`); empty drops
    /// it.
    prefix: &'a str,
    members: Vec<&'a Page>,
    per_page: usize,
    lang: &'a str,
    config: &'a Config,
}

impl<'a> Section<'a> {
    /// The index section for a collection, or `None` when it configures no
    /// index; without a `paginate { size }` every member sits on a single page.
    fn of(collection: &'a Collection, config: &'a Config, lang: &'a str) -> Option<Self> {
        let paginate = &collection.config.paginate;
        if !paginate.enabled {
            return None;
        }
        let members: Vec<&Page> = collection
            .pages
            .iter()
            .filter(|p| p.eligible(config) && p.listed(config) && p.lang == lang)
            .collect();
        let per_page = paginate.size.unwrap_or(members.len()).max(1);
        Some(Self {
            id: &collection.id,
            template: paginate.template.clone(),
            mount: collection.config.home(&collection.id),
            prefix: &paginate.prefix,
            members,
            per_page,
            lang,
            config,
        })
    }

    /// Push one page per chunk; an empty collection gets an index only in the
    /// default language, so a memberless `/fr/` is never listed.
    fn build(&self, out: &mut Vec<Page>) {
        if self.members.is_empty() && self.lang != self.config.lang {
            return;
        }
        let paged = Paged::new(
            std::slice::from_ref(&self.id),
            Some(&self.mount),
            self.prefix,
            "",
            self.per_page,
            self.lang,
            self.config,
        );
        let chunks = paged.chunks(&self.members);
        for (index, chunk) in chunks.iter().enumerate() {
            out.push(
                self.page(&paged, index + 1, chunk, chunks.len())
                    .into_page(self.config),
            );
        }
    }

    fn page(&self, paged: &Paged, number: usize, members: &[&Page], total: usize) -> Listing {
        let strings = Strings::new(self.config, self.lang);
        let items = members.iter().map(|p| Item::of(p, &strings)).collect();
        Listing::new(
            self.id,
            paged.slug(number),
            paged.url(number),
            paged.title(&Titlecase(self.id).to_string(), number),
        )
        .items(items)
        .nav(paged.nav(number, total))
        .template(self.template.clone())
        .lang(self.lang)
    }
}
