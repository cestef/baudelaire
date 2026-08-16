//! The site's link graph, as a build converges on it: backlinks are circular,
//! so a build guesses, compiles, and repairs what the guess got wrong.

use rayon::prelude::*;
use typst::syntax::Source;

#[cfg(feature = "markdown")]
use crate::content::Data;
use crate::content::Page;
use crate::error::Result;
use crate::graph::Cache;
use crate::render::{Backlinks, LinkMap, Outbound};
use crate::world::Project;

use super::pass::{Pass, Rendered, Reused};

/// The protocol a build settles its link graph by; the graph itself lives on
/// [`super::compile::prepare::Prepare`], which hands it to a compile.
pub(super) struct Graph;

impl Graph {
    /// How many times a build may recompile before it stops and reports the
    /// site unstable.
    const REPAIRS: usize = 2;

    /// What each page's backlinks are guessed to be before anything has
    /// rendered: the graph the last build recorded, and for a page it never saw,
    /// the one that page's source looks like it has. Neither half is trusted;
    /// [`Graph::settle`] checks both against the site the build renders.
    pub(super) fn predicted(
        project: &Project,
        links: &LinkMap,
        cache: &Cache,
        pages: &[Page],
        lang: bool,
    ) -> Backlinks {
        let edges: Vec<(&Page, Outbound)> = pages
            .par_iter()
            .filter_map(|page| match cache.recorded(page) {
                Some(recorded) => Some((page, recorded.clone())),
                None if !page.authored() => None,
                None => Some((
                    page,
                    Outbound::scanned(
                        &Self::scannable(project, page)?,
                        &page.source,
                        &page.permalink,
                        links,
                        lang.then_some(page.lang.as_str()),
                    ),
                )),
            })
            .collect();
        Backlinks::new(edges.iter().map(|(page, outbound)| (*page, outbound)))
    }

    /// The Typst a page's literal links are read out of: the file its author
    /// wrote, or, for a page whose source is not Typst, the Typst it lowered
    /// to. The scan looks for string literals, which a markdown page has none
    /// of where its links are.
    fn scannable(project: &Project, page: &Page) -> Option<Source> {
        match &page.data {
            #[cfg(feature = "markdown")]
            Data::Lowered { .. } => Some(Source::new(
                typst::syntax::FileId::new(project.virtualize(&page.source).ok()?),
                page.body.clone(),
            )),
            _ => project.source(&page.source).ok(),
        }
    }

    /// Make every page's backlinks true, compiling again the ones whose
    /// prediction the site disagreed with. Returns the pages that still
    /// disagree once the rounds are spent, which is a site to fix rather than a
    /// build to hang. `repair` recompiles the pages it is handed against the
    /// graph `pass` now assumes.
    pub(super) fn settle<'a>(
        pass: &mut Pass<'a>,
        rendered: &mut Vec<Rendered<'a>>,
        cached: &mut Vec<Reused<'a>>,
        mut repair: impl FnMut(&Pass<'a>, Vec<&'a Page>) -> Result<Vec<Rendered<'a>>>,
    ) -> Result<Vec<&'a Page>> {
        for _ in 0..Self::REPAIRS {
            let stale = Self::disagreeing(pass, rendered, cached);
            if stale.is_empty() {
                return Ok(Vec::new());
            }
            tracing::debug!(pages = stale.len(), "backlinks repaired");
            Self::absorb(repair(pass, stale)?, rendered, cached);
        }
        Ok(Self::disagreeing(pass, rendered, cached))
    }

    /// Assume the graph this build has produced, then name the pages that were
    /// not compiled against it. In that order, so the question asked is the one
    /// the next compile answers rather than one the page never sees.
    fn disagreeing<'a>(
        pass: &mut Pass<'a>,
        rendered: &[Rendered<'a>],
        cached: &[Reused<'a>],
    ) -> Vec<&'a Page> {
        pass.prepare
            .assume(Backlinks::new(Self::edges(rendered, cached)));
        let prepare = &pass.prepare;
        rendered
            .iter()
            .map(|r| (r.page, r.outputs.backlinks))
            .chain(cached.iter().map(|(page, _, out)| (*page, out.backlinks)))
            .filter(|&(page, was)| was != prepare.digest(page))
            .map(|(page, _)| page)
            .collect()
    }

    /// Every page's own outbound links, freshly compiled and cache-served alike:
    /// this build's link graph, which [`Backlinks`] inverts.
    fn edges<'a, 'r>(
        rendered: &'r [Rendered<'a>],
        cached: &'r [Reused<'a>],
    ) -> impl Iterator<Item = (&'a Page, &'r Outbound)> {
        rendered
            .iter()
            .map(|r| (r.page, &r.outputs.outbound))
            .chain(cached.iter().map(|(page, _, out)| (*page, &out.outbound)))
    }

    /// Put each repaired page back where it came from, so the next round reads
    /// this one's markup rather than the markup it replaced.
    fn absorb<'a>(
        repaired: Vec<Rendered<'a>>,
        rendered: &mut Vec<Rendered<'a>>,
        cached: &mut Vec<Reused<'a>>,
    ) {
        for mut page in repaired {
            if let Some(at) = rendered
                .iter()
                .position(|r| std::ptr::eq(r.page, page.page))
            {
                page.artifacts = std::mem::take(&mut rendered[at].artifacts);
                rendered[at] = page;
            } else {
                cached.retain(|(cached, ..)| !std::ptr::eq(*cached, page.page));
                rendered.push(page);
            }
        }
    }
}
