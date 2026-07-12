//! Content discovery and the site's page set.
//!
//! [`discover`] walks the content root into [`Collection`]s of [`Page`]s;
//! [`plan`] turns those into the full build set — eligible content pages plus
//! generated taxonomy and paginated index pages — with permalink uniqueness
//! enforced. Submodules own the pieces: frontmatter, permalinks, slugs,
//! listings, taxonomy, and pagination.

pub mod eval;
pub mod frontmatter;
pub mod listing;
pub mod page;
pub mod pagination;
pub mod permalink;
pub mod slug;
pub mod taxonomy;

pub use frontmatter::{Extract, Frontmatter};
pub use page::{Collection, Page, PageId, discover};
pub use slug::Slug;
pub use pagination::Pagination;
pub use permalink::{Permalink, PermalinkCtx, PermalinkError};
pub use taxonomy::Taxonomy;

use crate::config::Config;
use crate::error::{ContentError, Result};

/// The site's full page set: eligible content pages plus generated taxonomy and
/// paginated index pages, with permalink collisions rejected. The single entry
/// point the engine calls — all page-set assembly lives here, not in the engine.
pub fn plan(config: &Config) -> Result<Vec<Page>> {
    let collections = discover(config)?;
    let mut pages: Vec<Page> = collections
        .iter()
        .flat_map(|c| c.pages.iter())
        .filter(|p| p.eligible(config))
        .cloned()
        .collect();
    pages.extend(Taxonomy::pages(config, &pages)?);
    pages.extend(Pagination::pages(config, &collections));
    unique(&pages)?;
    Ok(pages)
}

/// Reject two pages that resolve to the same permalink — otherwise the second
/// silently overwrites the first's output (and clobbers its cache entry, which
/// is keyed by source). Catches colliding slugs, a `posts/index.typ` shadowing a
/// paginated `/posts/`, and nested files that flatten to one URL.
fn unique(pages: &[Page]) -> Result<()> {
    let mut seen: std::collections::HashMap<&str, &Page> = std::collections::HashMap::new();
    for page in pages {
        if let Some(prev) = seen.insert(&page.permalink, page) {
            return Err(ContentError::collision(
                &page.permalink,
                &prev.source.display().to_string(),
                &page.source.display().to_string(),
            )
            .into());
        }
    }
    Ok(())
}
