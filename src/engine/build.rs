//! One build's state, and the phases that advance it.
//!
//! [`Phase::ORDER`] is the single source of the pipeline's order: a phase is
//! one method on [`Build`] plus one row there, and nothing else decides what
//! happens when.

use crate::error::Result;
use crate::render::Emitted;
use crate::ui::{Dur, Ui};

use super::asset::{Assets, Deferred, Processed};
use super::compile::image::Images;
use super::compile::sidecar::{Artifact, Tally};
use super::emit::Output;
use super::pass::{Pass, Rendered, Reused};
use super::statics::Copied;
use super::summary::Summary;
use super::{Bundled, Engine, Generated, Planned, Stats};
use crate::graph::Cache;

/// One phase of a build.
///
/// [`Phase::ORDER`] is the pipeline: a phase is one variant, one row there, and
/// one arm of [`Phase::run`], so a phase added without being placed or
/// implemented does not compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Compile,
    Relink,
    Images,
    Owned,
    Validate,
    Write,
    Bundles,
    Artifacts,
    Publish,
    Generate,
    Save,
    Sweep,
}

impl Phase {
    /// The pipeline in order, each phase under the name a trace calls it by:
    /// compile what is stale, settle the link graph, copy what the pages
    /// carried, check the result, write it, then derive the whole-site files
    /// from it and sweep whatever no longer belongs.
    const ORDER: &'static [(&'static str, Self)] = &[
        ("compiling pages", Self::Compile),
        ("repairing backlinks", Self::Relink),
        ("copying images", Self::Images),
        ("writing requested assets", Self::Owned),
        ("checking pages", Self::Validate),
        ("writing pages", Self::Write),
        ("bundling documents", Self::Bundles),
        ("writing artifacts", Self::Artifacts),
        ("publishing assets", Self::Publish),
        ("generating files", Self::Generate),
        ("saving the cache", Self::Save),
        ("pruning", Self::Sweep),
    ];

    fn run(self, build: &mut Build<'_>, ui: &Ui) -> Result<()> {
        match self {
            Self::Compile => build.compile(ui),
            Self::Relink => build.relink(ui),
            Self::Images => build.images(ui),
            Self::Owned => build.owned(),
            Self::Validate => build.validate(ui),
            Self::Write => build.write(),
            Self::Bundles => build.bundles(ui),
            Self::Artifacts => build.artifacts(),
            Self::Publish => build.publish(),
            Self::Generate => build.generate(ui),
            Self::Save => build.save(),
            Self::Sweep => build.sweep(ui),
        }
    }
}

/// What the prologue produced, and what a build begins from: everything a
/// phase would otherwise have to build for itself, and everything it borrows
/// from outside the build.
pub(super) struct Prologue<'a> {
    pub(super) engine: &'a Engine,
    pub(super) planned: &'a Planned,
    pub(super) assets: &'a Assets<'a>,
    pub(super) pass: Pass<'a>,
    pub(super) statics: Copied,
    pub(super) processed: Processed,
    /// Warnings already on the counter, so the summary reports this build's own
    /// and not the session's.
    pub(super) warned: usize,
}

/// What a build has produced so far.
///
/// Everything one phase reads from an earlier one lives here. What a build
/// borrows from *outside* itself, and so cannot own without borrowing from its
/// own fields, is borrowed for `'a`: the engine, the plan the pass was wired
/// over, and the asset pipeline.
pub(super) struct Build<'a> {
    engine: &'a Engine,
    /// The plan this build's pass was wired over, for the phases that read the
    /// page set rather than what the pass made of it.
    planned: &'a Planned,
    assets: &'a Assets<'a>,
    /// The static tree this build staged, for the summary and the sweep.
    statics: Copied,
    pass: Pass<'a>,
    cache: Cache,
    /// What this build wrote and what each file digests to: the asset pipeline's
    /// output, then the images pages carried out of themselves.
    emitted: Emitted,
    /// The assets the pipeline named but did not write, until a page asks.
    deferred: Vec<Deferred>,
    /// What the asset pipeline itself wrote, for the summary.
    processed: (usize, u64),
    rendered: Vec<Rendered<'a>>,
    cached: Vec<Reused<'a>>,
    images: Images<'a>,
    /// The build's own assets that a page turned out to point at.
    owned: Processed,
    bundled: Bundled,
    generated: Generated,
    /// Warnings already on the counter when this build started, so the summary
    /// reports its own and not the session's.
    warned: usize,
}

impl<'a> Build<'a> {
    /// The pipeline in order: compile what is stale, settle the link graph,
    /// copy what the pages carried, check the result, write it, then derive the
    /// whole-site files from it and sweep what no longer belongs.
    /// Wire a build over what the prologue produced, and predict the backlinks
    /// pass one compiles against.
    pub(super) fn start(prologue: Prologue<'a>, ui: &Ui) -> Result<Self> {
        let Prologue {
            engine,
            planned,
            assets,
            mut pass,
            statics,
            processed,
            warned,
        } = prologue;
        let cache = engine.cached(&pass, planned, ui)?;
        engine.predict(&mut pass, &cache, planned);
        Ok(Self {
            images: engine.copier(),
            emitted: processed.emitted,
            deferred: processed.deferred,
            processed: (processed.count, processed.bytes),
            engine,
            planned,
            assets,
            statics,
            pass,
            cache,
            rendered: Vec::new(),
            cached: Vec::new(),
            owned: Processed::default(),
            bundled: Bundled::default(),
            generated: Generated::default(),
            warned,
        })
    }

    /// Run every phase in order, timing each: the one place the pipeline's
    /// order is walked.
    pub(super) fn advance(&mut self, ui: &Ui) -> Result<()> {
        for (name, phase) in Phase::ORDER {
            let timer = super::Timer::start();
            phase.run(self, ui)?;
            tracing::debug!(phase = name, elapsed = %Dur(timer.elapsed()), "phase");
        }
        Ok(())
    }

    /// Every built page, cached and freshly compiled alike.
    ///
    /// Rebuilt on each ask rather than held: an [`Output`] borrows the render
    /// results beside it, and one struct cannot hold both.
    fn outputs(&self) -> Vec<Output<'_>> {
        Engine::outputs(&self.rendered, &self.cached)
    }

    /// Every artifact drawn beside a page, and every bundle, for the same
    /// reason.
    fn artifacts_drawn(&self) -> Vec<&Artifact> {
        self.rendered
            .iter()
            .flat_map(|r| &r.artifacts)
            .chain(self.bundled.drawn.iter())
            .collect()
    }

    fn compile(&mut self, ui: &Ui) -> Result<()> {
        let (rendered, cached) = self.engine.incremental(&self.pass, &mut self.cache, ui)?;
        self.rendered = rendered;
        self.cached = cached;
        Ok(())
    }

    fn relink(&mut self, ui: &Ui) -> Result<()> {
        let Self {
            engine,
            pass,
            cache,
            rendered,
            cached,
            ..
        } = self;
        engine.relink(pass, cache, rendered, cached, ui)
    }

    /// Copy every page's externalized images into the (freshly regenerated)
    /// asset directory, for fresh and cache-served pages alike.
    fn images(&mut self, ui: &Ui) -> Result<()> {
        let Self {
            images,
            rendered,
            cached,
            emitted,
            ..
        } = self;
        images.copy(
            rendered
                .iter()
                .flat_map(|r| &r.outputs.images)
                .chain(cached.iter().flat_map(|(_, _, out)| &out.images)),
            ui,
        )?;
        emitted.absorb(images.emitted());
        Ok(())
    }

    fn owned(&mut self) -> Result<()> {
        let wanted = Engine::owned(&self.rendered, &self.cached);
        self.owned = self.assets.requested(&self.deferred, &wanted)?;
        Ok(())
    }

    fn validate(&self, ui: &Ui) -> Result<()> {
        self.engine
            .validate(&self.rendered, &self.cached, Some(&self.emitted), false, ui)
    }

    fn write(&self) -> Result<()> {
        Engine::write(&self.outputs())
    }

    fn bundles(&mut self, ui: &Ui) -> Result<()> {
        self.bundled = self.engine.bundles(&self.pass, &mut self.cache, ui)?;
        Ok(())
    }

    fn artifacts(&self) -> Result<()> {
        Engine::artifacts(&self.artifacts_drawn())
    }

    fn publish(&self) -> Result<()> {
        self.assets.publish()
    }

    fn generate(&mut self, ui: &Ui) -> Result<()> {
        let Self {
            engine,
            planned,
            statics,
            cache,
            rendered,
            cached,
            ..
        } = self;
        let outputs = Engine::outputs(rendered, cached);
        self.generated = engine.generate(planned, &outputs, statics, cache, ui)?;
        Ok(())
    }

    fn save(&self) -> Result<()> {
        self.cache
            .save(self.outputs().iter().map(|out| (out.page, out.html)))
    }

    fn sweep(&self, ui: &Ui) -> Result<()> {
        self.engine.sweep(
            ui,
            &self.outputs(),
            &self.statics,
            &self.generated,
            &self.bundled,
        )
    }

    /// Report what the build produced, and hand back what the caller counts.
    pub(super) fn finish(self, ui: &Ui, elapsed: std::time::Duration) -> Stats {
        ui.flush();
        let outputs = self.outputs();
        let artifacts = self.artifacts_drawn();
        let sidecars = Tally::of(artifacts.iter().copied());
        let total = self.rendered.len() + self.cached.len();
        let (asset_count, asset_bytes) = self.processed;
        Summary {
            pages: total,
            cached: self.cached.len(),
            assets: asset_count + self.images.count() + self.owned.count,
            statics: self.statics.count,
            generated: self.generated.count,
            bytes: outputs.iter().map(|out| out.html.len() as u64).sum::<u64>()
                + asset_bytes
                + self.owned.bytes
                + self.images.bytes()
                + self.generated.bytes
                + self.statics.bytes
                + sidecars.bytes,
            sidecars: sidecars.kinds,
            warnings: ui.warnings() - self.warned,
            dist: &self.engine.config.paths.dist,
            elapsed,
        }
        .report(ui);
        Stats {
            pages: total,
            cached: self.cached.len(),
            generated: self.generated.count,
            read: self.engine.outside(self.cache.read()),
        }
    }
}
