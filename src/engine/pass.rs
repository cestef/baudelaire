//! One compile pass over the site: what every page in it shares, how the page
//! set is split against the cache, and what a compiled page comes back as.

use rayon::prelude::*;

use crate::config::Config;
use crate::content::Page;
use crate::error::{Result, TypstSourceDiagnostic};
use crate::graph::{Analyzer, Cache, Deps, Hash, Outputs, Reads, Recorded, Root, Roots};
use crate::render::{
    AssetDeps, AssetMap, Emitted, LinkDeps, Renderer, SrcSetDeps, SrcSets, UrlDeps,
};

use super::compile::prepare::{Prepare, Prepared};
use super::compile::sidecar::{Artifact, Sidecars};
use super::{Engine, Planned};

/// A page served from the cache: the HTML the build that compiled it produced,
/// and the render-pass outputs recorded alongside.
pub(super) type Reused<'a> = (&'a Page, String, Outputs);

/// Everything a compile pass over the site shares: the pages it covers, their
/// compile inputs, the render layer they are rewritten through, and the
/// analyzer that records which injected values each one read. Built once and
/// consumed by both [`Engine::run`] and [`Engine::check`].
pub(super) struct Pass<'a> {
    pub(super) config: &'a Config,
    pub(super) pages: &'a [Page],
    pub(super) prepare: Prepare<'a>,
    pub(super) renderer: Renderer,
    pub(super) analyzer: Analyzer<'a>,
    /// The artifacts drawn beside each page's HTML, registered once for the
    /// pass rather than rebuilt per page inside the pool.
    pub(super) sidecars: Sidecars,
}

impl<'a> Pass<'a> {
    /// Wire a pass over `planned`, rendering against `assets`, `srcsets` and
    /// `emitted`: what the asset pipeline produced for a build, empty for a
    /// check, which rewrites nothing it will not write.
    pub(super) fn new(
        engine: &'a Engine,
        planned: &'a Planned,
        prepare: Prepare<'a>,
        assets: AssetMap,
        srcsets: SrcSets,
        emitted: Emitted,
    ) -> Self {
        Self {
            config: &engine.config,
            pages: &planned.pages,
            prepare,
            renderer: Renderer::new(crate::render::Inputs {
                pages: &planned.pages,
                entities: planned.entities.clone(),
                assets,
                srcsets,
                emitted,
                root: engine.project.root(),
                content: engine.config.paths.under(engine.project.root()).content,
                sources: engine.config.sources(),
            }),
            analyzer: Analyzer::new(
                planned.tracked.iter().map(Root::from).collect::<Roots>(),
                &engine.project,
            ),
            sidecars: Sidecars::builtin(),
        }
    }

    /// Split the pages into cache hits and stale ones. A page whose input could
    /// not be built is stale, so its error is reported by the compile pass with
    /// every other page's.
    pub(super) fn split(
        &self,
        cache: &mut Cache,
    ) -> (Vec<Reused<'a>>, Vec<(&'a Page, Result<Prepared>)>) {
        let prepared: Vec<(&'a Page, Result<Prepared>)> = self
            .pages
            .par_iter()
            .map(|page| (page, self.prepare.input(page)))
            .collect();
        let mut cached = Vec::new();
        let mut stale = Vec::new();
        for (page, input) in prepared {
            match input {
                Ok((id, text, fingerprint)) => {
                    match self
                        .drawn(page)
                        .then(|| cache.reuse(page, &fingerprint))
                        .flatten()
                    {
                        Some((html, outputs)) => cached.push((page, html, outputs)),
                        None => stale.push((page, Ok((id, text, fingerprint)))),
                    }
                }
                Err(e) => stale.push((page, Err(e))),
            }
        }
        (cached, stale)
    }

    /// Whether every file this page's sidecars own is still on disk. Only the
    /// build that compiles a page draws one, so a missing file has to make the
    /// page stale or nothing would ever draw it again.
    fn drawn(&self, page: &Page) -> bool {
        self.sidecars
            .planned(self.config, page)
            .iter()
            .all(|path| path.exists())
    }
}

/// A compiled page ready to write, with the files its compilation depended on,
/// the raw targets of any broken internal links it contained, and the warnings
/// typst raised while compiling it.
pub(super) struct Rendered<'a> {
    pub(super) page: &'a Page,
    pub(super) fingerprint: Hash,
    pub(super) html: String,
    pub(super) deps: Deps,
    pub(super) reads: Reads,
    /// The permalinks this page's links resolved against: a render-side
    /// dependency the compile itself never sees, since typst does not read a
    /// link target's source.
    pub(super) links: LinkDeps,
    /// The URLs this page's already-URL links named, and whether the site served
    /// a page at each: its dependency on the page set, which the compile does
    /// not see either.
    pub(super) urls: UrlDeps,
    /// The responsive variants this page's images matched: generated by the
    /// asset pipeline and matched render-side, so the compile never sees them.
    pub(super) srcsets: SrcSetDeps,
    /// The asset-map entries this page's references resolved through.
    pub(super) assets: AssetDeps,
    /// The render pass's own results (externalized images, broken links), the
    /// same shape the cache stores and replays for a hit.
    pub(super) outputs: Outputs,
    /// The files this page produces beside its HTML (a social card..), each
    /// with the destination it was drawn for.
    pub(super) artifacts: Vec<Artifact>,
    pub(super) warnings: Vec<TypstSourceDiagnostic>,
}

impl Rendered<'_> {
    /// The same page with its compile warnings dropped, so a backlink repair
    /// does not report the same source's warnings twice.
    pub(super) fn silenced(mut self) -> Self {
        self.warnings.clear();
        self
    }
}

/// The cache stores the subset of a compile that survives it; the rest is
/// consumed by this build alone.
impl<'a> From<&'a Rendered<'a>> for Recorded<'a> {
    fn from(rendered: &'a Rendered<'a>) -> Self {
        Self {
            page: rendered.page,
            fingerprint: rendered.fingerprint,
            html: &rendered.html,
            deps: &rendered.deps,
            reads: &rendered.reads,
            links: &rendered.links,
            urls: &rendered.urls,
            srcsets: &rendered.srcsets,
            assets: &rendered.assets,
            outputs: &rendered.outputs,
        }
    }
}
