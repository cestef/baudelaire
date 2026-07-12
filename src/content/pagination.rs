//! Pagination of collection index pages.
//!
//! A collection whose config sets `paginate = N` gets a chain of index
//! [`Listing`]s — `/{collection}/`, `/{collection}/page/2/`, … — each listing
//! `N` members with prev/next navigation.

use crate::config::Config;
use crate::content::listing::{Item, Listing, Nav, Titlecase};
use crate::content::{Collection, Page, Permalink};

/// Builds paginated index pages for a site's collections.
pub struct Pagination;

impl Pagination {
    /// Generate index pages for every collection configured with `paginate`,
    /// over its build-eligible members.
    pub fn pages(config: &Config, collections: &[Collection]) -> Vec<Page> {
        let mut out = Vec::new();
        for collection in collections {
            if let Some(per_page) = collection.config.paginate.filter(|n| *n > 0) {
                Section::new(collection, config, per_page).build(config, &mut out);
            }
        }
        out
    }
}

/// One collection's build-eligible members, chunked into pages.
struct Section<'a> {
    id: &'a str,
    template: Option<String>,
    /// Permalink of page 1 (`config.index`); later pages hang under it.
    index: Option<String>,
    members: Vec<&'a Page>,
    per_page: usize,
}

impl<'a> Section<'a> {
    fn new(collection: &'a Collection, config: &Config, per_page: usize) -> Self {
        let members = collection
            .pages
            .iter()
            .filter(|p| p.eligible(config))
            .collect();
        Self {
            id: &collection.id,
            template: collection.config.list.clone(),
            index: collection.config.index.clone(),
            members,
            per_page,
        }
    }

    fn build(&self, config: &Config, out: &mut Vec<Page>) {
        let chunks: Vec<&[&Page]> = self.members.chunks(self.per_page).collect();
        for (index, chunk) in chunks.iter().enumerate() {
            out.push(self.page(index + 1, chunk, chunks.len()).into_page(config));
        }
    }

    /// The listing for page `number` of `total`.
    fn page(&self, number: usize, members: &[&Page], total: usize) -> Listing {
        let items = members.iter().map(|p| Item::of(p)).collect();
        let title = match number {
            1 => Titlecase(self.id).to_string(),
            n => format!("{} — page {n}", Titlecase(self.id)),
        };
        Listing::new(self.id, Self::slug(number), self.url(number), title)
            .items(items)
            .nav(Nav {
                prev: (number > 1).then(|| self.url(number - 1)),
                next: (number < total).then(|| self.url(number + 1)),
            })
            .template(self.template.clone())
    }

    /// Page 1 lives at the collection root (or the configured `index`, e.g. `/`
    /// for a blog home); later pages under `page/{n}/`.
    fn url(&self, number: usize) -> String {
        match number {
            1 => self
                .index
                .clone()
                .unwrap_or_else(|| Permalink::join(&[self.id])),
            n => Permalink::join(&[self.id, "page", &n.to_string()]),
        }
    }

    fn slug(number: usize) -> String {
        match number {
            1 => "index".to_owned(),
            n => format!("page-{n}"),
        }
    }
}
