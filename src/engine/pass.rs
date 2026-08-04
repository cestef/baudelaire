//! One compile pass over the site: what every page in it shares, how the page
//! set is split against the cache, and what a compiled page comes back as.
//!
//! [`Pass`] is built once and consumed by both a build and a check, so the two
//! cannot drift apart in what they render against. [`Rendered`] is the other
//! end of the same story: everything one page's compile produced, of which the
//! cache keeps only the part that survives the build.

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
/// compile inputs, the render layer they are rewritten through, and the analyzer
/// that records which injected values each one read.
///
/// Built once and consumed by both [`Engine::run`] and [`Engine::check`], which
/// otherwise derived the same four values side by side and had to be kept in
/// step by hand.
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
            renderer: Renderer::new(
                &planned.pages,
                assets,
                srcsets,
                emitted,
                engine.project.root(),
                // Resolved here, once: it costs a canonicalization and every
                // page's links are tested against the same answer.
                engine.config.paths.under(engine.project.root()).content,
            ),
            analyzer: Analyzer::new(
                planned.tracked.iter().map(Root::from).collect::<Roots>(),
                &engine.project,
            ),
            sidecars: Sidecars::builtin(),
        }
    }

    /// Split the pages into cache hits and stale ones.
    ///
    /// [`Prepare`] produces each page's text and fingerprint without parsing it;
    /// the parse into a typst `Source` is deferred to the compile, so a hit
    /// never pays to parse a page it won't render. A page whose input could not
    /// be built is stale, so its error is reported by the compile pass with
    /// every other page's.
    pub(super) fn split(
        &self,
        cache: &mut Cache,
    ) -> (Vec<Reused<'a>>, Vec<(&'a Page, Result<Prepared>)>) {
        // Built across the pool first, because a page's input is pure and
        // independent of every other: it canonicalizes the page's path and
        // formats its wrapper text, which on a large site is the whole of a
        // serial prologue nothing else was waiting on. The probe below stays
        // ordered, since it mutates the cache and stages what it reuses.
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
                    // The existence check comes first so a page that has to be
                    // recompiled anyway leaves no reuse bookkeeping behind.
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

    /// Whether every file this page's sidecars own is still on disk.
    ///
    /// A page's HTML is rewritten on every build, hit or miss, because the cache
    /// holds the markup itself. A sidecar is not: only the build that compiles a
    /// page draws one, so once the file is gone (a deleted `dist`, a hand-removed
    /// card, a half-finished copy) nothing would ever draw it again and the cache
    /// would keep reporting the page as built. Missing one makes the page stale,
    /// which is the only thing that redraws it.
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
    /// Outbound `http(s)` links the page carries, for `check --external`. Not
    /// cached: only a fresh compile collects them, and only `check` reads them.
    pub(super) external: Vec<String>,
    /// The files this page produces beside its HTML (a social card..), each
    /// with the destination it was drawn for.
    pub(super) artifacts: Vec<Artifact>,
    pub(super) warnings: Vec<TypstSourceDiagnostic>,
}

impl Rendered<'_> {
    /// The same page with its compile warnings dropped.
    ///
    /// A backlink repair compiles the very same source a second time, so typst
    /// raises the very same warnings: reported again they would print twice and
    /// count twice in the build summary, telling an author there are more
    /// problems than there are.
    pub(super) fn silenced(mut self) -> Self {
        self.warnings.clear();
        self
    }
}

/// The cache stores the subset of a compile that survives it. The rest
/// (outbound links, the sidecar files, warnings) is consumed by this build alone.
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
