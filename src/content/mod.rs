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
pub mod entities;
pub mod frontmatter;
mod generate;
pub mod listing;
#[cfg(feature = "markdown")]
pub mod markdown;
pub mod page;
pub mod pagination;
pub mod section;
pub mod selection;
pub mod slug;
pub mod sourcemap;
mod stem;
pub mod strings;
pub mod taxonomy;

pub use cache::DiscoveryCache;
pub use date::{Iso, Localized};
pub use discovery::{Collection, ROOT, discover};
pub use entities::{Attribution, Byline, Credit, Entity, Registries, Registry, Resolved};
pub use frontmatter::{Frontmatter, Generated, Origin};
pub use page::{Data, Page, PageId, Sibling, Siblings, Withheld};
pub use pagination::Pagination;
pub use section::Section;
pub use selection::Selection;
pub use slug::Slug;
pub use sourcemap::{Rebased, SourceMap};
pub use strings::Strings;
pub use taxonomy::Taxonomy;

use crate::config::Config;
use crate::error::{ContentError, Result};
use crate::ui::markup;
use crate::world::Project;

/// How many pages discovery found that the build left out, by reason.
///
/// Counted rather than merely filtered, because a page that is not published is
/// something an author has to be told about. Three pages in and one page out
/// was a silent `built 2 pages`, with the strings `draft` and `expired` nowhere
/// in the output at any verbosity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub drafts: usize,
    pub future: usize,
    pub expired: usize,
}

impl Held {
    /// Tally what `collections` holds back under `config`.
    fn of(collections: &[discovery::Collection], config: &Config) -> Self {
        let (drafts, future) = (config.content.drafts.build, config.content.future);
        let mut held = Self::default();
        for page in collections.iter().flat_map(|c| c.pages.iter()) {
            match page.withheld(drafts, future) {
                Some(Withheld::Draft) => held.drafts += 1,
                Some(Withheld::Future) => held.future += 1,
                Some(Withheld::Expired) => held.expired += 1,
                None => {}
            }
        }
        held
    }

    /// Whether anything was held back at all.
    pub fn any(self) -> bool {
        self.drafts + self.future + self.expired > 0
    }
}

/// `2 drafts, 1 expired`: only the reasons that apply, each with its count, so
/// the line reads as a sentence in the diagnostic that carries it.
impl std::fmt::Display for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let counts = [
            (self.drafts, "draft", "drafts"),
            (self.future, "future-dated page", "future-dated pages"),
            (self.expired, "expired page", "expired pages"),
        ];
        let mut first = true;
        for (count, one, many) in counts {
            if count == 0 {
                continue;
            }
            if !std::mem::take(&mut first) {
                f.write_str(", ")?;
            }
            let noun = if count == 1 { one } else { many };
            write!(f, "{count} {noun}")?;
        }
        Ok(())
    }
}

/// The site's full page set: eligible content pages plus generated taxonomy and
/// paginated index pages, with permalink collisions rejected. The single entry
/// point the engine calls: all page-set assembly lives here, not in the engine.
pub fn plan(config: &Config, project: &Project) -> Result<Plan> {
    let collections = discover(config, project)?;
    let held = Held::of(&collections, config);
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
            // A pager links the pages a reader navigates between, which the
            // not-found page is not: it neither appears in a neighbour's pager
            // nor gets neighbours of its own.
            let (linked, rest): (Vec<&Page>, Vec<&Page>) =
                group.into_iter().partition(|p| p.listed(config));
            for (i, page) in linked.iter().enumerate() {
                let mut page = (*page).clone();
                page.siblings = page::Siblings {
                    prev: i.checked_sub(1).map(|j| linked[j].sibling()),
                    next: linked.get(i + 1).map(|n| n.sibling()),
                };
                pages.push(page);
            }
            pages.extend(rest.into_iter().cloned());
        }
    }
    // The registries every reference resolves against, built from the content
    // snapshot before anything derives pages from it: a `pages` source draws
    // its entities from pages the plan has already read, and a generated
    // listing is never one of them.
    let entities = Registries::build(config, project, &pages)?;
    // Synthetic pages (taxonomy indexes, paginated listings) derive from the
    // content snapshot above; each generator runs against the same `pages`.
    let generated = generate::Generators::builtin().generate(&generate::PlanCtx {
        config,
        entities: &entities,
        pages: &pages,
        collections: &collections,
    })?;
    pages.extend(generated);
    Page::relate(&mut pages, config);
    Claim::unique(&pages, config)?;
    Ok(Plan {
        pages,
        held,
        entities,
    })
}

/// What planning produced: the pages a build renders, what it left out, and the
/// entity registries every one of them resolves references against.
///
/// A struct rather than a tuple, because the registries are a third thing the
/// engine carries the length of a build and a two-tuple was already one thing
/// too many to read at the call site.
pub struct Plan {
    pub pages: Vec<Page>,
    pub held: Held,
    pub entities: Registries,
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
        let claims = pages
            .iter()
            .flat_map(|page| Self::of(page, config))
            .chain(Self::declared(config));
        for claim in claims {
            if let Some(first) = seen.insert(claim.output.clone(), claim.origin.clone()) {
                return Err(ContentError::collision(
                    &claim.output.display().to_string(),
                    &first,
                    &claim.origin,
                )
                .into());
            }
        }
        Ok(())
    }

    /// Every file the config's own `redirect { }` pairs will write.
    ///
    /// An old path with no page behind it is still a file in `dist`, so it
    /// belongs in the same accounting as the pages: a pair aimed at a path some
    /// page does own would otherwise bury that page under a stub forwarding
    /// away from it, and the site would lose a page to a config line. Last, so
    /// a page is always the claim reported as the first.
    ///
    /// A wildcard pair claims nothing: it is written as a rule and never as a
    /// file, so reserving `dist/latest/*/index.html` for it would be accounting
    /// for a file no build writes, and colliding over one.
    fn declared(config: &Config) -> impl Iterator<Item = Self> + '_ {
        config
            .redirect
            .iter()
            .filter(|(old, _)| !Config::wildcard(old))
            .map(|(old, _)| Self {
                output: config.destination(old),
                origin: markup!("`redirect {{ \"{}\" }}` in the config", old),
            })
    }

    /// Every file `page` will write: its own HTML, plus one stub per
    /// frontmatter `redirect` entry.
    fn of<'a>(page: &'a Page, config: &'a Config) -> impl Iterator<Item = Self> + 'a {
        let own = Self {
            output: page.output.clone(),
            origin: page.source.display().to_string(),
        };
        // Localized exactly as the emitter localizes it, or this check answers a
        // question the build never asks. Translating a page by copying its
        // frontmatter carries the `redirect` list along, and each edition
        // forwards an old path under its own language prefix: `/old/a/` and
        // `/fr/old/a/` are two files. Compared unlocalized they looked like one,
        // and the documented workflow failed the build on a collision that does
        // not exist.
        let stubs = page
            .frontmatter
            .redirect
            .iter()
            .filter(|old| !Config::wildcard(old))
            .map(|old| {
                let old = config.localize(&page.lang, old);
                Self {
                    output: config.destination(&old),
                    origin: markup!("`redirect \"{}\"` in {}", old, page.source.display()),
                }
            });
        std::iter::once(own).chain(stubs)
    }
}
