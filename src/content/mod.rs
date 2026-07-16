//! Content discovery and the site's page set.
//!
//! [`discover`] walks the content root into [`Collection`]s of [`Page`]s;
//! [`plan`] turns those into the full build set (eligible content pages plus
//! generated taxonomy and paginated index pages) with permalink uniqueness
//! enforced. Submodules own the pieces: frontmatter, permalinks, slugs,
//! listings, taxonomy, and pagination.

pub mod cache;
pub mod date;
pub mod frontmatter;
mod generate;
pub mod listing;
pub mod page;
pub mod pagination;
pub mod permalink;
pub mod section;
pub mod slug;
pub mod taxonomy;

pub use cache::DiscoveryCache;
pub use date::Iso;
pub use frontmatter::Frontmatter;
pub use page::{Collection, Data, Page, PageId, Sibling, Siblings, discover};
pub use pagination::Pagination;
pub use permalink::{Permalink, PermalinkCtx, PermalinkError};
pub use section::Section;
pub use slug::Slug;
pub use taxonomy::Taxonomy;

use crate::config::Config;
use crate::error::{ContentError, Result};
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
        for (i, page) in eligible.iter().enumerate() {
            let mut page = (*page).clone();
            page.siblings = page::Siblings {
                prev: i.checked_sub(1).map(|j| eligible[j].sibling()),
                next: eligible.get(i + 1).map(|n| n.sibling()),
            };
            pages.push(page);
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
    unique(&pages, config)?;
    Ok(pages)
}

/// Reject two claimants of one output file; otherwise the second silently
/// overwrites the first. Keyed on the destination *file*, not the permalink
/// string: [`Config::destination`] normalizes segments, so distinct permalinks
/// can still meet on disk. Covers colliding slugs, a `posts/index.typ`
/// shadowing a paginated `/posts/`, nested files that flatten to one URL, and
/// a redirect stub aimed at a real page's file.
fn unique(pages: &[Page], config: &Config) -> Result<()> {
    let mut seen: std::collections::HashMap<std::path::PathBuf, String> =
        std::collections::HashMap::new();
    for page in pages {
        for claim in Claim::of(page, config) {
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

/// One claim on an output file, and where it came from, the single accounting
/// of everything a page writes into `dist`.
struct Claim {
    output: std::path::PathBuf,
    origin: String,
}

impl Claim {
    /// Every file `page` will write: its own HTML, plus one stub per
    /// frontmatter `redirect` entry.
    fn of<'a>(page: &'a Page, config: &'a Config) -> impl Iterator<Item = Self> + 'a {
        let own = Self {
            output: page.output.clone(),
            origin: page.source.display().to_string(),
        };
        let stubs = page.frontmatter.redirect.iter().map(|old| Self {
            output: config.destination(old),
            origin: format!("`redirect \"{old}\"` in {}", page.source.display()),
        });
        std::iter::once(own).chain(stubs)
    }
}
