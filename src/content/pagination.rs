//! Pagination of collection index pages.
//!
//! A collection whose config sets `paginate = N` gets a chain of index
//! [`Listing`]s — `/{collection}/`, `/{collection}/page/2/`, … — each listing
//! `N` members with prev/next navigation.

use crate::config::Config;
use crate::content::listing::{Item, Listing, Nav};
use crate::content::{Collection, Page};

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
            1 => Listing::titlecase(self.id),
            n => format!("{} — page {n}", Listing::titlecase(self.id)),
        };
        Listing::new(self.id, Self::slug(number), self.url(number), title)
            .items(items)
            .nav(Nav {
                prev: (number > 1).then(|| self.url(number - 1)),
                next: (number < total).then(|| self.url(number + 1)),
            })
            .template(self.template.clone())
    }

    /// Page 1 lives at the collection root; later pages under `page/{n}/`.
    fn url(&self, number: usize) -> String {
        match number {
            1 => format!("/{}/", self.id),
            n => format!("/{}/page/{n}/", self.id),
        }
    }

    fn slug(number: usize) -> String {
        match number {
            1 => "index".to_owned(),
            n => format!("page-{n}"),
        }
    }
}
