//! Build pipeline: discover -> compile -> render -> write, parallelized via rayon.

pub(crate) mod asset;
mod check;
mod compile;
pub(crate) mod emit;
pub(crate) mod gate;
mod hook;
mod layers;
mod links;
mod pass;
mod prune;
mod statics;
mod summary;
pub mod text;

use std::path::PathBuf;
use std::sync::Arc;

use rayon::prelude::*;
use tracing::{debug, trace};
use typst::syntax::{FileId, Source};
use typst_html::{HtmlDocument, HtmlOptions};

use crate::codegen::Value;
use crate::config::Config;
use crate::content::{Data, Page, Plan};
use crate::engine::asset::Assets;
#[cfg(feature = "js")]
use crate::engine::asset::JsCtx;
use crate::engine::check::External;
use crate::engine::check::{Budgets, CheckedPage, Compiled, Links, Lints, Orphans};
#[cfg(any(feature = "pdf", feature = "epub"))]
use crate::engine::compile::bundle::Bundle;
use crate::engine::compile::image::Images;
use crate::engine::compile::prepare::{Prepare, Prepared};
use crate::engine::compile::sidecar::{Artifact, Sidecars, Tally};
use crate::engine::emit::{Emitter, Output, Processors, Site};
use crate::engine::gate::{Gate, Inert};
use crate::engine::hook::Hooks;
use crate::engine::links::Graph;
use crate::engine::pass::{Pass, Rendered, Reused};
use crate::engine::statics::{Copied, Static};
use crate::engine::summary::Summary;
use crate::error::warning::{BacklinksUnstable, FeatureMissing, SettingInert};
use crate::error::{BaudelaireErrorKind, BuildFailed, ConfigError, Result, TypstSourceDiagnostic};
use crate::fs;
use crate::generated::Generated as _;
use crate::graph::{Cache, Hash, Outputs, SiteInputs};
use crate::render::{AssetMap, Emitted, Fragments, SrcSets, Syndicated};
use crate::theme::Theme;
use crate::ui::{Count, Dur, PageStatus, Timer, Ui};
pub use crate::world::Mode;
use crate::world::{PageWorld, Project, Tracked};

/// Build statistics returned to callers.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub pages: usize,
    pub cached: usize,
    /// Whole-site files this build wrote. The ones a skipped processor left
    /// standing are not among them: they were not written.
    pub generated: usize,
    /// The directories holding files this build read from outside its own source
    /// trees, for the dev server to watch. Directories rather than files, so a
    /// file created beside a tracked one is seen too.
    pub read: Vec<PathBuf>,
}

/// The bundled documents a build dealt with: the ones it exported, and every
/// one the site asks for. `paths` comes from the config and the page set, never
/// from what was written: a cached bundle produces no artifact, and the sweep
/// would drop it.
#[derive(Default)]
struct Bundled {
    drawn: Vec<Artifact>,
    paths: Vec<PathBuf>,
}

/// What the post-build processors emitted, for the summary and the prune.
struct Generated {
    count: usize,
    bytes: u64,
    paths: Vec<PathBuf>,
}

/// The build engine: owns shared project state and drives the pipeline.
pub struct Engine {
    project: Project,
    config: Config,
    /// What this run is for, which decides where its incremental state lives.
    mode: Mode,
    /// The resolved theme, when the site names one.
    theme: Option<Theme>,
    /// What this binary cannot do that the site asked for, from [`Gate`].
    gaps: Vec<FeatureMissing>,
    /// What the site asked for that its own config withholds, from [`Inert`].
    inert: Vec<SettingInert>,
}

impl Engine {
    pub fn new(config: Config, mode: Mode) -> Result<Self> {
        if let Some((key, source)) = config.paths.swallowed(&config.root) {
            return Err(ConfigError::dist_contains_source(
                &config.paths.dist,
                key,
                &source.display().to_string(),
            )
            .into());
        }
        if let Some(dir) = config.typst.fonts.missing(&config.root) {
            return Err(ConfigError::missing_font_dir(dir).into());
        }
        let (config, gaps) = Gate::resolve(config);
        let inert = Inert::resolve(&config);
        let theme = Theme::of(&config)?;
        let project = Project::new(&config, mode, theme.as_ref())?;
        Ok(Self {
            project,
            config,
            mode,
            theme,
            gaps,
            inert,
        })
    }

    /// Build the site incrementally: reuse cached output for unchanged pages,
    /// recompile the rest in parallel, then copy assets. Failure leaves `dist`
    /// as the previous build left it, staging tree removed, which `deploy`
    /// would otherwise upload as a duplicate copy of the assets.
    pub fn build(&self, ui: &Ui) -> Result<Stats> {
        self.project.refresh();
        let built = self.run(ui);
        if built.is_err() {
            let _ = std::fs::remove_dir_all(self.config.asset_staging());
        }
        built
    }

    /// Whether this engine may build again: its world holds build metadata
    /// fixed at construction, and a page that read stale metadata would be
    /// compiled against a value the site no longer states.
    pub fn current(&self) -> bool {
        self.project.current(&self.config, self.mode)
    }

    /// A build, phase by phase; this is the only place their order is spelled.
    fn run(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        let statics = self.staged(ui)?;
        let planned = self.planned("planned build", ui)?;
        let warned = ui.warnings();
        for gap in &self.gaps {
            ui.warn(*gap);
        }
        for inert in &self.inert {
            ui.warn(*inert);
        }
        let has_not_found = planned.pages.iter().any(|page| !page.listed(&self.config));
        if !has_not_found {
            ui.warn(crate::error::warning::NotFoundMissing);
        }
        let hooks = Hooks::new(&self.config);
        hooks.before(ui)?;
        let prepare = self.prepared(&planned, ui)?;
        #[cfg(feature = "js")]
        let modules = Modules::new(self, &prepare);
        let assets = Assets::new(
            &self.config,
            self.theme.as_ref(),
            #[cfg(feature = "js")]
            modules.ctx(&planned.pages),
        );
        let mut processed = Self::processed(&assets, ui)?;
        let deferred = std::mem::take(&mut processed.deferred);
        let (asset_count, asset_bytes) = (processed.count, processed.bytes);
        let mut emitted = processed.emitted;
        let mut pass = Pass::new(
            self,
            &planned,
            prepare,
            processed.map,
            processed.srcsets,
            emitted.clone(),
        );
        let mut cache = self.cached(&pass, &planned, ui)?;
        if self.config.links.backlinks {
            pass.prepare.assume(Graph::predicted(
                &self.project,
                pass.renderer.maps().links,
                &cache,
                &planned.pages,
                self.config.multilingual(),
            ));
        }
        let (mut rendered, mut cached) = self.incremental(&pass, &mut cache, ui)?;
        self.relink(&mut pass, &mut cache, &mut rendered, &mut cached, ui)?;
        let images = self.images(&rendered, &cached, ui)?;
        emitted.absorb(images.emitted());
        let owned = assets.requested(&deferred, &Self::owned(&rendered, &cached))?;
        self.validate(&rendered, &cached, Some(&emitted), false, ui)?;
        let outputs = Self::outputs(&rendered, &cached);
        Self::write(&outputs)?;
        let bundled = self.bundles(&pass, &mut cache, ui)?;
        let artifacts: Vec<&Artifact> = rendered
            .iter()
            .flat_map(|r| &r.artifacts)
            .chain(bundled.drawn.iter())
            .collect();
        Self::artifacts(&artifacts)?;
        assets.publish()?;
        let generated = self.generate(&planned, &outputs, &statics, &mut cache, ui)?;
        cache.save(outputs.iter().map(|out| (out.page, out.html)))?;
        self.sweep(ui, &outputs, &statics, &generated, &bundled)?;
        hooks.after(ui)?;

        ui.flush();
        let total = rendered.len() + cached.len();
        let sidecars = Tally::of(artifacts.iter().copied());
        Summary {
            pages: total,
            cached: cached.len(),
            assets: asset_count + images.count() + owned.count,
            statics: statics.count,
            generated: generated.count,
            bytes: outputs.iter().map(|out| out.html.len() as u64).sum::<u64>()
                + asset_bytes
                + owned.bytes
                + images.bytes()
                + generated.bytes
                + statics.bytes
                + sidecars.bytes,
            sidecars: sidecars.kinds,
            warnings: ui.warnings() - warned,
            dist: &self.config.paths.dist,
            elapsed: timer.elapsed(),
        }
        .report(ui);
        Ok(Stats {
            pages: total,
            cached: cached.len(),
            generated: generated.count,
            read: self.outside(cache.read()),
        })
    }

    /// The directories holding `files`, minus anything already inside a source
    /// tree the dev server watches by default. Deduped and sorted, so a rebuild
    /// that read the same files hands back the same list.
    fn outside(&self, files: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
        let watched = self
            .config
            .paths
            .trees()
            .map(|(_, dir)| crate::fs::canonical(dir));
        let mut dirs: Vec<PathBuf> = files
            .filter_map(|file| file.parent().map(std::path::Path::to_path_buf))
            .map(|dir| crate::fs::canonicalize(&dir).unwrap_or(dir))
            .filter(|dir| !watched.iter().any(|root| dir.starts_with(root)))
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
    }

    /// Prepare `dist` and seed it with the static tree, which goes down first
    /// so a generated page or asset at the same output path overwrites it.
    fn stage(&self) -> Result<Copied> {
        fs::create_dir_all(&self.config.paths.dist)?;
        let _ = std::fs::remove_dir_all(self.config.asset_staging());
        let statics = Static::new(&self.config, self.theme.as_ref()).copy()?;
        debug!(
            count = statics.count,
            bytes = statics.bytes,
            "static copied"
        );
        Ok(statics)
    }

    /// Plan the pages this pass covers, alongside the tracked value trees every
    /// consumer of them borrows. `what` names the pass in the trace line.
    fn planned(&self, what: &'static str, ui: &Ui) -> Result<Planned> {
        let planned = {
            let _step = ui.step("reading content");
            Plan::of(&self.config, &self.project)?
        };
        debug!(
            pages = planned.pages.len(),
            site = self.config.label(),
            "{what}"
        );
        if planned.held.any() {
            ui.advice(crate::error::warning::PagesHeld(planned.held));
        }
        planned
            .entities
            .check(&self.config, &self.project, &planned.pages, ui)?;
        Ok(Planned {
            pages: planned.pages,
            relations: planned.relations,
            entities: planned.entities,
            tracked: self.project.tracked(),
        })
    }

    /// The compile inputs for `pages`: the wrapper text binding each page to its
    /// template, plus the section trees derived from the whole page set, which
    /// are written out here so a template's `#import` of one resolves on the
    /// first compile of a fresh checkout.
    fn prepare<'a>(&'a self, planned: &'a Planned) -> Result<Prepare<'a>> {
        let prepare = Prepare::new(
            &self.config,
            &self.project,
            self.theme.as_ref(),
            &planned.pages,
            &planned.entities,
            &planned.relations,
            self.project.history(),
        );
        let root = self.project.root();
        for table in prepare.generated() {
            table.write(root)?;
        }
        #[cfg(feature = "js")]
        crate::engine::asset::Declarations::of(&self.config).write(root)?;
        self.project.tables_written();
        Ok(prepare)
    }

    /// Copy the static tree, which is one phase however many files it holds.
    fn staged(&self, ui: &Ui) -> Result<Copied> {
        let _step = ui.step("staging static files");
        self.stage()
    }

    /// Build the compile inputs, and refuse the ones that cannot stand.
    fn prepared<'a>(&'a self, planned: &'a Planned, ui: &Ui) -> Result<Prepare<'a>> {
        let _step = ui.step("preparing pages");
        let prepare = self.prepare(planned)?;
        prepare.verify()?;
        Ok(prepare)
    }

    /// Read the manifest of the last run of this mode.
    fn cached(&self, pass: &Pass, planned: &Planned, ui: &Ui) -> Result<Cache> {
        let _step = ui.step("reading the cache");
        self.cache(pass, planned, ui)
    }

    /// Run the asset pipeline, which is one phase however many files it walks.
    fn processed(assets: &Assets<'_>, ui: &Ui) -> Result<crate::engine::asset::Processed> {
        let _step = ui.step("processing assets");
        let processed = assets.process()?;
        debug!(
            count = processed.count,
            bytes = processed.bytes,
            "assets processed"
        );
        Ok(processed)
    }

    /// Load the build cache, keyed on every site-wide input the per-page
    /// dependency tracker cannot see.
    fn cache(&self, pass: &Pass, planned: &Planned, ui: &Ui) -> Result<Cache> {
        let inputs = SiteInputs {
            modules: self.project.modules(),
            fonts: self.project.fonts(),
        };
        Cache::load(
            &self.config,
            &inputs,
            planned.tracked.clone(),
            pass.renderer.maps(),
            self.project.root(),
            self.mode.cache(&self.config.cache.dir),
            ui,
        )
    }

    /// Serve what the cache still covers, compile the rest, and record every
    /// fresh page against the manifest the next build reads.
    fn incremental<'a>(
        &self,
        pass: &Pass<'a>,
        cache: &mut Cache,
        ui: &Ui,
    ) -> Result<(Vec<Rendered<'a>>, Vec<Reused<'a>>)> {
        let (cached, stale) = pass.split(cache);
        debug!(stale = stale.len(), reused = cached.len(), "cache split");
        let rendered = self.render_pages("compiling", stale, ui, |(page, prepared)| {
            (
                page,
                prepared.and_then(|(id, text, fp)| self.compile(page, id, text, fp, pass)),
            )
        })?;
        for r in &rendered {
            cache.record(r.into());
        }
        for (page, _, _) in &cached {
            ui.page(self.relative(page), PageStatus::Cached);
        }
        Ok((rendered, cached))
    }

    /// Make every page's backlinks true, compiling again the ones the site
    /// disagreed with, and warn if the graph never settles. A repaired page
    /// keeps the sidecars pass one drew for it.
    fn relink<'a>(
        &self,
        pass: &mut Pass<'a>,
        cache: &mut Cache,
        rendered: &mut Vec<Rendered<'a>>,
        cached: &mut Vec<Reused<'a>>,
        ui: &Ui,
    ) -> Result<()> {
        if !self.config.links.backlinks {
            return Ok(());
        }
        pass.sidecars = Sidecars::none();
        let unstable = Graph::settle(pass, rendered, cached, |pass, stale| {
            let inputs: Vec<(&'a Page, Result<Prepared>)> = stale
                .into_iter()
                .map(|page| (page, pass.prepare.input(page)))
                .collect();
            let repaired = self.render_pages("relinking", inputs, ui, |(page, prepared)| {
                (
                    page,
                    prepared
                        .and_then(|(id, text, fp)| self.compile(page, id, text, fp, pass))
                        .map(Rendered::silenced),
                )
            })?;
            for page in &repaired {
                cache.relink(page.into());
            }
            Ok(repaired)
        })?;
        if !unstable.is_empty() {
            ui.warn(BacklinksUnstable {
                pages: unstable.iter().map(|page| self.relative(page)).collect(),
            });
        }
        Ok(())
    }

    /// Copy every page's externalized images into the (freshly regenerated)
    /// asset directory, for fresh and cache-served pages alike.
    fn images(&self, rendered: &[Rendered], cached: &[Reused], ui: &Ui) -> Result<Images<'_>> {
        Images::new(&self.config, self.project.root()).copy(
            rendered
                .iter()
                .flat_map(|r| &r.outputs.images)
                .chain(cached.iter().flat_map(|(_, _, out)| &out.images)),
            ui,
        )
    }

    /// The build's own assets that any page points at, rendered and cache-served
    /// alike.
    fn owned(rendered: &[Rendered], cached: &[Reused]) -> std::collections::BTreeSet<String> {
        rendered
            .iter()
            .flat_map(|r| &r.outputs.owned)
            .chain(cached.iter().flat_map(|(_, _, out)| &out.owned))
            .cloned()
            .collect()
    }

    /// Pair every page, rendered and cache-served alike, with what the render
    /// pass produced for it.
    fn outputs<'a>(rendered: &'a [Rendered<'a>], cached: &'a [Reused<'a>]) -> Vec<Output<'a>> {
        rendered
            .iter()
            .map(|r| Output {
                page: r.page,
                html: r.html.as_str(),
                fragments: r.outputs.fragments.as_ref(),
                syndicated: r.outputs.syndicated.as_ref(),
                inline: &r.outputs.inline,
            })
            .chain(cached.iter().map(|(page, html, out)| Output {
                page,
                html: html.as_str(),
                fragments: out.fragments.as_ref(),
                syndicated: out.syndicated.as_ref(),
                inline: &out.inline,
            }))
            .collect()
    }

    /// Write every page's HTML in parallel: independent files, no shared state.
    fn write(outputs: &[Output]) -> Result<()> {
        outputs
            .par_iter()
            .try_for_each(|out| fs::write_all(&out.page.output, out.html))
    }

    /// Write every artifact this build produced beside the pages: the sidecars
    /// drawn during compile, and the bundled documents. Only what was freshly
    /// made, since a cache hit leaves the previous build's file in place.
    fn artifacts(artifacts: &[&Artifact]) -> Result<()> {
        artifacts
            .par_iter()
            .try_for_each(|artifact| fs::write_all(&artifact.path, &artifact.bytes))
    }

    /// Export the bundled documents this site asks for: a collection, or the
    /// whole site, as one PDF. A bundle belongs to no page, so it carries a
    /// cache entry of its own, keyed on the module text and what its compile
    /// read.
    #[cfg(feature = "pdf")]
    fn bundles(&self, pass: &Pass<'_>, cache: &mut Cache, ui: &Ui) -> Result<Bundled> {
        let mut bundled = Bundled {
            paths: Bundle::claimed(&self.config, pass.pages),
            ..Bundled::default()
        };
        for bundle in Bundle::planned(&self.config, pass.pages) {
            if !bundle.typeset() {
                continue;
            }
            let id = bundle.id();
            let path = bundle.path(&self.config);
            let text = bundle.source(&pass.prepare, &self.project)?;
            let fingerprint = Hash::of_bytes(text.as_bytes());
            if cache.reuse_bundle(&id, &fingerprint, &path) {
                debug!(bundle = %id, "bundle reused");
                continue;
            }
            let (bytes, deps) = bundle.export(&self.project, &pass.prepare, text)?;
            cache.record_bundle(&id, fingerprint, &deps);
            ui.page(bundle.label(), PageStatus::Built);
            bundled.drawn.push(Artifact {
                kind: Bundle::KIND,
                path,
                bytes,
            });
        }
        Ok(bundled)
    }

    /// Without the exporter nothing is typeset, but the files an emitted format
    /// writes are still the site's, so the sweep still has to be told about
    /// them. The signature mirrors the `pdf`-on one so the caller compiles
    /// unchanged in both flavors.
    #[cfg(not(feature = "pdf"))]
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn bundles(&self, _pass: &Pass<'_>, _cache: &mut Cache, _ui: &Ui) -> Result<Bundled> {
        #[cfg(feature = "epub")]
        return Ok(Bundled {
            paths: Bundle::claimed(&self.config, _pass.pages),
            ..Bundled::default()
        });
        #[cfg(not(feature = "epub"))]
        Ok(Bundled::default())
    }

    /// Run the post-build processors over the finished site.
    fn generate(
        &self,
        planned: &Planned,
        outputs: &[Output],
        statics: &Copied,
        cache: &mut Cache,
        ui: &Ui,
    ) -> Result<Generated> {
        let site = Site {
            config: &self.config,
            pages: &planned.pages,
            entities: &planned.entities,
            relations: &planned.relations,
            outputs,
            history: self.project.history(),
        };
        let mut emitter = Emitter::new(ui, statics.paths.iter().cloned());
        Processors::builtin().run(&site, &mut emitter, cache)?;
        Ok(Generated {
            count: emitter.written(),
            bytes: emitter.bytes(),
            paths: emitter.paths(),
        })
    }

    /// Drop orphaned outputs from earlier builds (a removed page or taxonomy
    /// term, a renamed permalink) so `dist` never serves stale files. A build
    /// that produced no page at all sweeps nothing, whatever `prune` says: an
    /// empty keep-set is more often a mistyped content path than a site with no
    /// pages.
    fn sweep(
        &self,
        ui: &Ui,
        outputs: &[Output],
        statics: &Copied,
        generated: &Generated,
        bundled: &Bundled,
    ) -> Result<()> {
        if !self.config.prune.enabled {
            return Ok(());
        }
        if outputs.is_empty() {
            ui.warn(crate::error::warning::PruneEmpty);
            return Ok(());
        }
        let sidecars = Sidecars::builtin();
        let drawn = outputs
            .iter()
            .flat_map(|out| sidecars.planned(&self.config, out.page));
        let keep: Vec<PathBuf> = outputs
            .iter()
            .map(|out| out.page.output.clone())
            .chain(drawn)
            .chain(bundled.paths.iter().cloned())
            .chain(statics.paths.iter().cloned())
            .chain(generated.paths.iter().cloned())
            .collect();
        let pruned = prune::Prune::new(
            &self.config.paths.dist,
            &self.config.asset_dist(),
            &self.config.cache.dir,
            &self.config.prune.keep,
        )?
        .run(&keep)?;
        debug!(pruned, "orphaned outputs removed");
        Ok(())
    }

    /// Compile what changed, report diagnostics, and write no output.
    ///
    /// Incremental like a build, against a manifest of its own: this renders
    /// without the asset pipeline, so its markup is not the markup a build
    /// writes and the two must not read each other's entries.
    pub fn check(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        let planned = self.planned("planned check", ui)?;
        let pass = Pass::new(
            self,
            &planned,
            self.prepared(&planned, ui)?,
            AssetMap::new(self.config.asset_prefix()),
            SrcSets::default(),
            Emitted::default(),
        );
        let mut cache = self.cached(&pass, &planned, ui)?;
        let (rendered, cached) = self.incremental(&pass, &mut cache, ui)?;
        self.validate(&rendered, &cached, None, true, ui)?;
        let outputs = Self::outputs(&rendered, &cached);
        cache.save(outputs.iter().map(|out| (out.page, out.html)))?;
        ui.flush();
        let total = rendered.len() + cached.len();
        ui.done(format_args!(
            "checked {} in {}",
            Count::pages(total),
            Dur(timer.elapsed())
        ));
        Ok(Stats {
            pages: total,
            cached: cached.len(),
            generated: 0,
            read: Vec::new(),
        })
    }

    /// Compile a batch of pages in parallel and reduce to their rendered
    /// outputs. The shared spine of `build` (the stale subset) and `check`
    /// (every page); `outcome` supplies the only difference, how one item
    /// renders.
    fn render_pages<'a, T: Send>(
        &self,
        label: &'static str,
        items: Vec<T>,
        ui: &Ui,
        outcome: impl Fn(T) -> (&'a Page, Result<Rendered<'a>>) + Sync,
    ) -> Result<Vec<Rendered<'a>>> {
        let progress = ui.progress(label, items.len());
        let outcomes: Vec<(&Page, Result<Rendered>)> = items
            .into_par_iter()
            .map(|item| {
                let (page, out) = outcome(item);
                progress.tick(self.relative(page));
                (page, out)
            })
            .collect();
        progress.finish();
        self.collect(outcomes, ui)
    }

    /// Report each compile outcome and return the rendered pages, or, once every
    /// failure has been reported, an error carrying all failed pages'
    /// diagnostics.
    fn collect<'a>(
        &self,
        outcomes: Vec<(&'a Page, Result<Rendered<'a>>)>,
        ui: &Ui,
    ) -> Result<Vec<Rendered<'a>>> {
        let mut errors = Vec::new();
        let mut rendered = Vec::new();
        for (page, outcome) in outcomes {
            match outcome {
                Ok(mut r) => {
                    ui.page(self.relative(page), PageStatus::Built);
                    for warning in r.warnings.drain(..) {
                        ui.warn(warning);
                    }
                    rendered.push(r);
                }
                Err(e) => {
                    ui.page(self.relative(page), PageStatus::Failed);
                    errors.push(e);
                }
            }
        }
        BuildFailed::aggregate(errors).map_or_else(|| Ok(rendered), Err)
    }

    /// A page's source path relative to the content root, for display. Handles
    /// both discovered pages (content-relative) and generated pages (whose
    /// synthetic sources are canonical-absolute).
    fn relative(&self, page: &Page) -> String {
        let canonical = fs::canonical(&self.config.paths.content);
        page.source
            .strip_prefix(canonical)
            .or_else(|_| page.source.strip_prefix(&self.config.paths.content))
            .unwrap_or(&page.source)
            .display()
            .to_string()
    }

    /// Compile a single page to rendered HTML, applying render post-processing
    /// (link rewriting over the typed DOM) before serialization.
    ///
    /// The recorded dependencies cover what the render pass and the page's
    /// sidecars read on top of what typst opened, plus the clock, since none of
    /// those show up in the compilation's own accesses. Typst's blanket "html
    /// export is under active development" warning is filtered out: HTML is
    /// this tool's entire output.
    fn compile<'a>(
        &self,
        page: &'a Page,
        id: FileId,
        text: String,
        fingerprint: Hash,
        pass: &Pass<'_>,
    ) -> Result<Rendered<'a>> {
        let timer = Timer::start();
        let source = Source::new(id, text);
        let world = Tracked::new(self.project.world_for(&source));
        let compiled = typst::compile::<HtmlDocument>(&world);
        let warnings = compiled
            .warnings
            .into_iter()
            .filter(|w| {
                !w.message
                    .starts_with("html export is under active development")
            })
            .collect();
        let warnings = Self::diagnostics(warnings, page, &source, world.inner());
        let mut doc = compiled.output.map_err(|errs| {
            BaudelaireErrorKind::TypstCompile(Self::diagnostics(errs, page, &source, world.inner()))
        })?;
        let mut rewrite = pass.renderer.rewrite(
            &mut doc,
            page,
            pass.relations.of(page),
            &self.config,
            world.inner(),
        );
        if let Some(invalid) = std::mem::take(&mut rewrite.invalid).into_iter().next() {
            return Err(invalid);
        }
        let options = HtmlOptions {
            pretty: self.config.pretty(),
        };
        let serialization_failed = |errs| {
            BaudelaireErrorKind::TypstHtml(Self::diagnostics(errs, page, &source, world.inner()))
        };
        let html = typst_html::html(&doc, &options).map_err(&serialization_failed)?;
        let fragments = self
            .config
            .navigation
            .standalone
            .enabled
            .then(|| Fragments::capture(&doc, &options).map_err(&serialization_failed))
            .transpose()?;
        let syndicated = (self.config.generate.feed.full() || self.config.binds_prose())
            .then(|| {
                Syndicated::capture(
                    &doc,
                    &options,
                    &self.config.html.region,
                    self.config.base().as_ref(),
                )
                .map_err(&serialization_failed)
            })
            .transpose()?;
        let (artifacts, drawn) =
            pass.sidecars
                .draw(&self.project, &self.config, &pass.prepare, page)?;
        trace!(
            page = %page.source.display(),
            bytes = html.len(),
            sidecars = artifacts.len(),
            elapsed = ?timer.elapsed(),
            "compiled"
        );
        let mut deps = self.project.dependencies(&world);
        deps.extend(std::mem::take(&mut rewrite.read));
        deps.extend(drawn.files().iter().cloned());
        let mut reads = pass.analyzer.reads(&source, &deps);
        if world.reads_clock() {
            reads.insert(Project::clock());
        }
        Ok(Rendered {
            page,
            fingerprint,
            html,
            deps,
            reads,
            links: rewrite.links,
            urls: rewrite.urls,
            srcsets: rewrite.srcsets,
            assets: rewrite.assets,
            outputs: Outputs {
                images: rewrite.images,
                owned: rewrite.owned,
                broken: rewrite.broken,
                anchors: rewrite.anchors,
                deep: rewrite.deep,
                outbound: rewrite.outbound,
                backlinks: pass.prepare.digest(page),
                fragments,
                syndicated,
                external: rewrite.external,
                lints: rewrite.lints,
                weight: rewrite.weight,
                inline: rewrite.inline,
            },
            artifacts,
            warnings,
        })
    }

    /// Run the post-render validation passes over every page, compiled and
    /// cached alike, a cache hit replaying the links it was built with so the
    /// gate does not weaken on rebuild. `outbound` reaches the network and so is
    /// passed only by [`Engine::check`]; a build stays offline whatever the
    /// config says.
    fn validate(
        &self,
        rendered: &[Rendered],
        cached: &[(&Page, String, Outputs)],
        emitted: Option<&Emitted>,
        outbound: bool,
        ui: &Ui,
    ) -> Result<()> {
        let fresh = rendered
            .iter()
            .map(|r| (r.page, r.html.as_str(), &r.outputs));
        let reused = cached
            .iter()
            .map(|(page, html, outputs)| (*page, html.as_str(), outputs));
        let pages: Vec<CheckedPage> = fresh
            .chain(reused)
            .map(|(page, html, outputs)| CheckedPage {
                label: self.relative(page),
                source: &page.source,
                permalink: &page.permalink,
                broken: &outputs.broken,
                external: &outputs.external,
                anchors: &outputs.anchors,
                deep: &outputs.deep,
                lints: &outputs.lints,
                weight: &outputs.weight,
                html,
                outbound: &outputs.outbound,
                lists: match &page.data {
                    Data::Generated { lists, .. } => lists,
                    _ => &[],
                },
                generated: !page.authored(),
                listed: page.listed(&self.config),
            })
            .collect();
        let site = Compiled {
            config: &self.config,
            pages: &pages,
            emitted,
        };
        Links::run(&site, ui)?;
        Orphans::run(&site, ui);
        if self.config.check.enabled {
            Lints::run(&site, ui)?;
            Budgets::run(&site, ui)?;
        }
        if outbound && self.config.check.external.enabled {
            External::run(&site, ui)?;
        }
        Ok(())
    }

    /// Wrap typst source diagnostics with the compiled source so miette renders
    /// spans against exactly what was compiled, mapping a lowered page's spans
    /// back to the lines its author wrote.
    fn diagnostics(
        errs: typst::ecow::EcoVec<typst::diag::SourceDiagnostic>,
        page: &Page,
        source: &Source,
        world: &PageWorld,
    ) -> Vec<TypstSourceDiagnostic> {
        TypstSourceDiagnostic::bridge(
            errs,
            (&page.source.display().to_string(), source.text()),
            Arc::new(world.clone()),
            {
                let named = page.source.display().to_string();
                let mapped: Option<&Arc<crate::content::SourceMap>> = match &page.data {
                    #[cfg(feature = "markdown")]
                    crate::content::Data::Lowered { sourcemap, .. } => Some(sourcemap),
                    _ => None,
                };
                mapped.map(|sourcemap| (sourcemap, named))
            }
            .as_ref()
            .map(|(sourcemap, name)| (*sourcemap, name.as_str())),
        )
    }
}

/// The owned inputs one pass over the site borrows: the planned pages and the
/// build's tracked value trees. Separate from [`Pass`] because [`Pass`] holds
/// borrows of both, and no struct can borrow from itself.
struct Planned {
    pages: Vec<Page>,
    /// Where each page sits among the others: the siblings it is compiled
    /// between, the editions of it the sitemap names.
    relations: crate::content::Relations,
    /// The entity registries every page's references resolve against, built
    /// once per plan and borrowed by the renderer and the emitters.
    entities: crate::content::Registries,
    /// The injected values whose per-page reads drive fine-grained metadata
    /// invalidation: the analyzer records them from each page's syntax, the
    /// cache re-hashes them to decide reuse.
    tracked: Vec<(String, Value)>,
}

/// The site values the `baudelaire:*` virtual JS modules serve, built from the
/// same wrapper inputs the templates get. Owned by the build, because
/// [`JsCtx`] borrows them for as long as the asset pipeline lives.
#[cfg(feature = "js")]
struct Modules {
    /// The codegen `Value` view of the build context; Typst reads `sys.inputs`
    /// from the raw context instead.
    context: Value,
    /// The section trees keyed by language: one bundle serves the whole site, so
    /// every translation needs its own tree.
    sections: Value,
}

#[cfg(feature = "js")]
impl Modules {
    fn new(engine: &Engine, prepare: &Prepare) -> Self {
        Self {
            sections: Value::dict(
                engine
                    .config
                    .langs()
                    .into_iter()
                    .map(|lang| (lang.to_owned(), prepare.sections(lang))),
            ),
            context: Value::from(engine.project.context()),
        }
    }

    /// The bundler's view of this build, completed with the page set.
    fn ctx<'a>(&'a self, pages: &'a [Page]) -> JsCtx<'a> {
        JsCtx {
            pages,
            context: &self.context,
            sections: &self.sections,
        }
    }
}
