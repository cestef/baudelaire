//! Collection index pages, optionally paginated.
//!
//! A collection that configures a `list` template or a `paginate = N` count
//! gets a generated index [`Listing`] at `/{collection}/`. With `paginate` its
//! members are chunked across `/{collection}/`, `/{collection}/page/2/`, .. each
//! with prev/next navigation; without it, all members sit on the single index
//! page. A collection that configures neither gets no index at all.

use crate::config::{Config, Permalink};
use crate::content::generate::{Generate, PlanCtx};
use crate::content::listing::{Item, Listing, Nav, Titlecase};
use crate::content::{Collection, Page, Strings};
use crate::error::Result;

/// A membership chunked into numbered pages, with the URL and slug rules that
/// follow from where the listing sits.
///
/// THE pagination rule, so the two things that paginate cannot disagree about
/// what page 2 is called. A collection index and a taxonomy term listing differ
/// only in what they list and where page 1 sits, and only the first of them
/// used to chunk at all: a blog with three years of `#rust` posts rendered all
/// 400 on one term page, beside a collection index that paginated the very same
/// pages.
pub(crate) struct Paged<'a> {
    /// The segments page 2 and later hang under, unlocalized: `["blog"]` for a
    /// collection, `["tags", "rust"]` for a term.
    root: &'a [&'a str],
    /// Where page 1 sits, unlocalized. `None` puts it at `root`, which is what
    /// a taxonomy term does; a collection passes
    /// [`CollectionConfig::home`](crate::config::CollectionConfig::home), the
    /// single answer to where that collection lives.
    mount: Option<&'a str>,
    /// Path segment before the number (`/blog/page/2/`); empty drops it.
    prefix: &'a str,
    /// The slug page 1 takes within its section; later pages extend it. Empty
    /// means the section index.
    slug: &'a str,
    /// Members per page. Never zero, since `chunks(0)` panics: a listing that
    /// does not paginate passes its whole membership.
    per_page: usize,
    lang: &'a str,
    config: &'a Config,
}

impl<'a> Paged<'a> {
    /// The page-number segment when no `prefix` names one. A slug has to stay a
    /// valid identifier, so it cannot simply be the bare number.
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

    /// The members split across pages. Never empty: a memberless listing still
    /// gets its page 1, since nav links point at it and an empty listing beats
    /// a 404.
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
        let word = match self.prefix.is_empty() {
            true => Self::WORD,
            false => self.prefix,
        };
        match (number, self.slug) {
            (1, "") => Listing::INDEX.to_owned(),
            (1, slug) => slug.to_owned(),
            (n, "") => format!("{word}-{n}"),
            (n, slug) => format!("{slug}-{word}-{n}"),
        }
    }

    /// Prev/next for page `number` of `total`.
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
    /// Generate index pages for every collection that asks for one (via a
    /// `list` template or a `paginate` count) over its build-eligible members.
    /// Never fails; the `Result` satisfies the shared [`Generate`] signature.
    fn generate(&self, ctx: &PlanCtx) -> Result<Vec<Page>> {
        let mut out = Vec::new();
        for collection in ctx.collections {
            // One index per language: members are partitioned by language, so a
            // collection's `/blog/` and `/fr/blog/` list only their own pages.
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
    /// Path segment before a page number (`/{id}/{prefix}/{n}/`); empty drops it.
    prefix: &'a str,
    members: Vec<&'a Page>,
    per_page: usize,
    /// The language this index covers; localizes every URL.
    lang: &'a str,
    config: &'a Config,
}

impl<'a> Section<'a> {
    /// The index section for a collection, or `None` when it configures no
    /// index. The `paginate { }` block's presence is what generates one; its
    /// `size` chunks the members, and without one every member sits on a single
    /// page (`per_page` = the whole membership).
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
        // A single un-paginated page holds every member; guard against a zero
        // chunk size for an empty collection (`chunks(0)` panics).
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

    fn build(&self, out: &mut Vec<Page>) {
        // Only the default language gets an index for an empty collection: a
        // memberless language simply has no listing rather than an empty /fr/.
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

    /// The listing for page `number` of `total`.
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
