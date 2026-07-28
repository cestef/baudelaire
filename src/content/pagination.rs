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
    /// Permalink of page 1 ([`crate::config::CollectionConfig::index`]); later
    /// pages hang under it.
    index: Option<String>,
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
    /// index. `paginate` sets the page size; a `list` template without one puts
    /// every member on a single page (`per_page` = the whole membership).
    fn of(collection: &'a Collection, config: &'a Config, lang: &'a str) -> Option<Self> {
        let paginate = collection.config.paginate.filter(|n| *n > 0);
        if paginate.is_none() && collection.config.list.is_none() {
            return None;
        }
        let members: Vec<&Page> = collection
            .pages
            .iter()
            .filter(|p| p.eligible(config) && p.lang == lang)
            .collect();
        // A single un-paginated page holds every member; guard against a zero
        // chunk size for an empty collection (`chunks(0)` panics).
        let per_page = paginate.unwrap_or(members.len()).max(1);
        Some(Self {
            id: &collection.id,
            template: collection.config.list.clone(),
            index: collection.config.index.clone(),
            prefix: &collection.config.prefix,
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
        let mut chunks: Vec<&[&Page]> = self.members.chunks(self.per_page).collect();
        // A memberless collection still gets its page-1 index: nav links point
        // at it, and an empty listing beats a 404.
        if chunks.is_empty() {
            chunks.push(&[]);
        }
        for (index, chunk) in chunks.iter().enumerate() {
            out.push(
                self.page(index + 1, chunk, chunks.len())
                    .into_page(self.config),
            );
        }
    }

    /// The listing for page `number` of `total`.
    fn page(&self, number: usize, members: &[&Page], total: usize) -> Listing {
        let items = members.iter().map(|p| Item::of(p)).collect();
        let title = match number {
            1 => Titlecase(self.id).to_string(),
            n => format!(
                "{} - {} {n}",
                Titlecase(self.id),
                Strings::new(self.config, self.lang).get("page")
            ),
        };
        Listing::new(self.id, self.slug(number), self.url(number), title)
            .items(items)
            .nav(Nav {
                prev: (number > 1).then(|| self.url(number - 1)),
                next: (number < total).then(|| self.url(number + 1)),
            })
            .template(self.template.clone())
            .lang(self.lang)
    }

    /// Page 1 lives at the collection root (or the configured `index`, e.g. `/`
    /// for a blog home); later pages under `{prefix}/{n}/`, or just `{n}/` when
    /// `prefix` is empty. Every URL is localized to the section's language.
    fn url(&self, number: usize) -> String {
        let raw = match number {
            1 => self
                .index
                .clone()
                .unwrap_or_else(|| Permalink::join(&[self.id])),
            n => {
                let num = n.to_string();
                let segments: &[&str] = if self.prefix.is_empty() {
                    &[self.id, &num]
                } else {
                    &[self.id, self.prefix, &num]
                };
                Permalink::join(segments)
            }
        };
        self.config.localize(self.lang, &raw)
    }

    /// The internal page slug (its [`PageId`] within the section, not a URL). A
    /// non-empty `prefix` names it (`page-2`); an empty one falls back to `page`
    /// so the slug stays a valid, collision-free identifier.
    fn slug(&self, number: usize) -> String {
        match number {
            1 => "index".to_owned(),
            n => {
                let word = if self.prefix.is_empty() {
                    "page"
                } else {
                    self.prefix
                };
                format!("{word}-{n}")
            }
        }
    }
}
