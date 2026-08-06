//! The site's link graph, as a build converges on it.
//!
//! Backlinks are circular: a page compiles *with* the pages whose content links
//! to it, and which pages those are is known only once every page has compiled.
//! A build therefore guesses, compiles, and repairs what the guess got wrong.
//!
//! This module owns that protocol end to end: the guess, the fold from compiled
//! pages back to a graph, the test for which pages disagree with it, and the
//! bound on how many rounds a build may spend. [`super::Engine`] is left with
//! the one part that is genuinely its own, recompiling a page.

use rayon::prelude::*;
use typst::syntax::Source;

// Only the lowered arm names `Data` now that authorship is asked of the page
// itself, and that arm is markdown's.
#[cfg(feature = "markdown")]
use crate::content::Data;
use crate::content::Page;
use crate::error::Result;
use crate::graph::Cache;
use crate::render::{Backlinks, LinkMap, Outbound};
use crate::world::Project;

use super::pass::{Pass, Rendered, Reused};

/// The site's link graph as a build settles it.
///
/// A namespace rather than a value: the graph being settled lives on
/// [`super::compile::prepare::Prepare`], which is what hands it to a compile,
/// and duplicating it here would be two copies of one fact. What belongs to
/// this type is the protocol around it, which nothing else may spell.
pub(super) struct Graph;

impl Graph {
    /// How many times a build may recompile before it stops and reports the
    /// site unstable. Two: the first repair settles every site whose content
    /// does not branch on its own backlinks, and the second exists to notice
    /// that it did not rather than to keep trying.
    const REPAIRS: usize = 2;

    /// What each page's backlinks are guessed to be before anything has
    /// rendered: the graph the last build recorded, and for a page it never saw,
    /// the one that page's source looks like it has.
    ///
    /// The second half is what makes a *cold* build cheap. With the manifest
    /// alone the guess on a first build was that nothing linked anywhere, so
    /// every page with an inbound link was compiled twice; scanning the sources
    /// gets the ordinary site (whose links are written out literally) right on
    /// the first pass. Neither half is trusted: [`Graph::settle`] checks both
    /// against the site the build actually renders.
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
                // A generated listing contributes no edges at all, so there is
                // nothing to scan it for; see the render transform.
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
    /// wrote, or, for a page whose source is not Typst at all, the Typst it
    /// lowered to, which the engine is already holding.
    ///
    /// The scan looks for string literals, and a markdown page has none where
    /// its links are: `[B](b.typ)` is content followed by text, not an
    /// `ast::Str`. Parsed as Typst, every `.md` page was predicted to link
    /// nowhere, and every one of them cost a repair round on a cold build.
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
    /// build to hang.
    ///
    /// `repair` recompiles the pages it is handed against the graph `pass` now
    /// assumes. It is the caller's because compiling is the caller's; every
    /// question of *which* pages and *how many times* is answered here.
    ///
    /// Repairing changes what a page's own content links to only if that content
    /// branches on its backlinks, so the graph re-settles at once in every
    /// ordinary site.
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
        // Only what is *still* wrong: the last round may have settled the site,
        // in which case there is nothing to say. Reporting regardless read "0
        // still disagree" and, under `--strict`, failed a build that converged.
        Ok(Self::disagreeing(pass, rendered, cached))
    }

    /// Assume the graph this build has produced, then name the pages that were
    /// not compiled against it.
    ///
    /// In that order, deliberately: the question asked is the one the next
    /// compile answers, "what would this page be compiled with now, and is that
    /// what it was compiled with?". Asking the graph directly instead put pages
    /// that cannot carry backlinks at all (a listing with no template of its
    /// own) permanently at odds with a value they never see.
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
                // The sidecars pass one drew for it, which a repair does not
                // redraw and the build still has to write.
                page.artifacts = std::mem::take(&mut rendered[at].artifacts);
                rendered[at] = page;
            } else {
                // A cache hit that had to be recompiled stops being one. Its
                // sidecars are already on disk, which is what let it be a hit.
                cached.retain(|(cached, ..)| !std::ptr::eq(*cached, page.page));
                rendered.push(page);
            }
        }
    }
}
