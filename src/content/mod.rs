//! Content discovery and the site's page set.
//!
//! [`discover`] walks the content root into [`Collection`]s of [`Page`]s;
//! [`plan`] turns those into the full build set (eligible content pages plus
//! generated taxonomy and paginated index pages) with permalink uniqueness
//! enforced. Submodules own the pieces: frontmatter, permalinks, slugs,
//! listings, taxonomy, and pagination.

pub mod cache;
pub mod date;
pub mod discovery;
pub mod frontmatter;
mod generate;
pub mod listing;
pub mod page;
pub mod pagination;
pub mod section;
pub mod slug;
mod stem;
pub mod strings;
pub mod taxonomy;

pub use cache::DiscoveryCache;
pub use date::{Iso, Localized};
pub use discovery::{Collection, discover};
pub use frontmatter::Frontmatter;
pub use page::{Data, Page, PageId, Sibling, Siblings};
pub use pagination::Pagination;
pub use section::Section;
pub use slug::Slug;
pub use strings::Strings;
pub use taxonomy::Taxonomy;

use crate::config::Config;
use crate::error::{ContentError, Result};
use crate::ui::markup;
use crate::world::Project;

/// The site's full page set: eligible content pages plus generated taxonomy and
/// paginated index pages, with permalink collisions rejected. The single entry
/// point the engine calls: all page-set assembly lives here, not in the engine.
pub fn plan(config: &Config, project: &Project) -> Result<Vec<Page>> {
    let collections = discover(config, project)?;
    // Within each collection, the eligible pages sit in the collection's sort
    // order; adjacent ones become each other's prev/next siblings (a blog's
    // older/newer links). Computed per collection so navigation never crosses a
    // boundary, and before taxonomy/pagination pages join the set.
    let mut pages: Vec<Page> = Vec::new();
    for collection in &collections {
        let eligible: Vec<&Page> = collection
            .pages
            .iter()
            .filter(|p| p.eligible(config))
            .collect();
        // Siblings link within a language as well as a collection: prev/next
        // never cross a language boundary. A single-language site is one group.
        for group in Page::groups(&eligible) {
            for (i, page) in group.iter().enumerate() {
                let mut page = (*page).clone();
                page.siblings = page::Siblings {
                    prev: i.checked_sub(1).map(|j| group[j].sibling()),
                    next: group.get(i + 1).map(|n| n.sibling()),
                };
                pages.push(page);
            }
        }
    }
    // Synthetic pages (taxonomy indexes, paginated listings) derive from the
    // content snapshot above; each generator runs against the same `pages`.
    let generated = generate::Generators::builtin().generate(&generate::PlanCtx {
        config,
        pages: &pages,
        collections: &collections,
    })?;
    pages.extend(generated);
    Page::relate(&mut pages, config);
    Claim::unique(&pages, config)?;
    Ok(pages)
}

/// One claim on an output file, and where it came from, the single accounting
/// of everything a page writes into `dist`.
struct Claim {
    output: std::path::PathBuf,
    origin: String,
}

impl Claim {
    /// Reject two claimants of one output file; otherwise the second silently
    /// overwrites the first. Keyed on the destination *file*, not the permalink
    /// string: [`Config::destination`] normalizes segments, so distinct
    /// permalinks can still meet on disk. Covers colliding slugs, a
    /// `posts/index.typ` shadowing a paginated `/posts/`, nested files that
    /// flatten to one URL, and a redirect stub aimed at a real page's file.
    fn unique(pages: &[Page], config: &Config) -> Result<()> {
        let mut seen: std::collections::HashMap<std::path::PathBuf, String> =
            std::collections::HashMap::new();
        for page in pages {
            for claim in Self::of(page, config) {
                if let Some(first) = seen.insert(claim.output.clone(), claim.origin.clone()) {
                    return Err(ContentError::collision(
                        &claim.output.display().to_string(),
                        &first,
                        &claim.origin,
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    /// Every file `page` will write: its own HTML, plus one stub per
    /// frontmatter `redirect` entry.
    fn of<'a>(page: &'a Page, config: &'a Config) -> impl Iterator<Item = Self> + 'a {
        let own = Self {
            output: page.output.clone(),
            origin: page.source.display().to_string(),
        };
        let stubs = page.frontmatter.redirect.iter().map(|old| Self {
            output: config.destination(old),
            origin: markup!("`redirect \"{}\"` in {}", old, page.source.display()),
        });
        std::iter::once(own).chain(stubs)
    }
}
