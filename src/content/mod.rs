//! Content discovery and the site's page set: [`discover`] walks the content
//! root into [`Collection`]s of [`Page`]s, and [`plan`] turns those into the
//! full build set.

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
pub use discovery::{Collection, Discovery, ROOT};
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

    pub fn any(self) -> bool {
        self.total() > 0
    }

    /// How many pages were held back, whatever the reason.
    pub fn total(self) -> usize {
        self.drafts + self.future + self.expired
    }
}

/// `2 drafts, 1 expired`: only the reasons that apply, each with its count.
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

/// What planning produced: the pages a build renders, what it left out, and the
/// entity registries every one of them resolves references against.
pub struct Plan {
    pub pages: Vec<Page>,
    pub held: Held,
    pub entities: Registries,
}

impl Plan {
    /// The site's full page set: eligible content pages plus generated taxonomy and
    /// paginated index pages, with permalink collisions rejected.
    pub fn of(config: &Config, project: &Project) -> Result<Self> {
        let collections = Discovery::all(config, project)?;
        let held = Held::of(&collections, config);
        for collection in &collections {
            tracing::debug!(
                collection = collection.id,
                pages = collection.pages.len(),
                "collected"
            );
        }
        let mut pages: Vec<Page> = Vec::new();
        for collection in &collections {
            let eligible: Vec<&Page> = collection
                .pages
                .iter()
                .filter(|p| p.eligible(config))
                .collect();
            for group in Page::groups(&eligible) {
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
        let entities = Registries::build(config, project, &pages)?;
        let generated = generate::Generators::builtin().generate(&generate::PlanCtx {
            config,
            entities: &entities,
            pages: &pages,
            collections: &collections,
        })?;
        tracing::debug!(
            generated = generated.len(),
            authored = pages.len(),
            held = held.total(),
            "planned"
        );
        pages.extend(generated);
        Page::relate(&mut pages, config);
        Claim::unique(&pages, config)?;
        Ok(Self {
            pages,
            held,
            entities,
        })
    }
}

/// One claim on an output file, and where it came from.
struct Claim {
    output: std::path::PathBuf,
    /// Where the claim was written, already marked up: the collision
    /// diagnostic interpolates it as-is.
    origin: String,
}

impl Claim {
    /// Reject two claimants of one output file; otherwise the second silently
    /// overwrites the first. Keyed on the destination *file*, not the permalink
    /// string: [`Config::destination`] normalizes segments, so distinct
    /// permalinks can still meet on disk.
    fn unique(pages: &[Page], config: &Config) -> Result<()> {
        let mut seen: std::collections::HashMap<std::path::PathBuf, String> =
            std::collections::HashMap::new();
        let claims = pages
            .iter()
            .flat_map(|page| Self::of(page, config))
            .chain(Self::declared(config))
            .chain(Self::generated(config));
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

    /// Every whole-site file a post-build processor will write, so a page
    /// slugged `sitemap.xml` is refused rather than quietly replaced by the
    /// sitemap that runs after it.
    fn generated(config: &Config) -> impl Iterator<Item = Self> + '_ {
        crate::engine::emit::Processors::claimed(config)
            .into_iter()
            .map(|(output, by)| Self {
                output,
                origin: markup!("{}", by),
            })
    }

    /// Every file the config's own `redirect { }` pairs will write; a wildcard
    /// pair claims nothing, being written as a rule and never as a file.
    fn declared(config: &Config) -> impl Iterator<Item = Self> + '_ {
        config
            .redirects
            .rules
            .iter()
            .filter(|(old, _)| !Config::wildcard(old))
            .map(|(old, _)| Self {
                output: config.destination(old),
                origin: markup!("`redirects {{ rules {{ \"{}\" }} }}` in the config", old),
            })
    }

    /// Every file `page` will write: its own HTML, plus one stub per
    /// frontmatter `redirect` entry, each localized exactly as the emitter
    /// localizes it or this check compares paths the build never writes.
    fn of<'a>(page: &'a Page, config: &'a Config) -> impl Iterator<Item = Self> + 'a {
        let own = Self {
            output: page.output.clone(),
            origin: markup!("`{}`", page.source.display()),
        };
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
