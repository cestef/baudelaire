//! Build pipeline: discover -> compile -> render -> write, parallelized via rayon.

pub(crate) mod asset;
mod check;
mod compile;
mod emit;
mod gate;
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
use tracing::debug;
use typst::syntax::{FileId, Source};
use typst_html::{HtmlDocument, HtmlOptions};

use crate::codegen::Value;
use crate::config::Config;
use crate::content::{Data, Page, plan};
use crate::engine::asset::Assets;
#[cfg(feature = "js")]
use crate::engine::asset::JsCtx;
use crate::engine::check::External;
use crate::engine::check::{Budgets, CheckedPage, Compiled, Links, Lints, Orphans};
#[cfg(feature = "pdf")]
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
// The trait only: this module has a `Generated` of its own, naming the
// post-build outputs rather than the files a build writes for tooling.
use crate::generated::Generated as _;
use crate::graph::{Cache, Hash, Outputs, RenderInputs};
use crate::render::{AssetMap, Emitted, Fragments, SrcSets};
use crate::theme::Theme;
use crate::ui::{Count, Dur, PageStatus, Timer, Ui};
pub use crate::world::Mode;
use crate::world::{PageWorld, Project, Tracked};

/// Build statistics returned to callers (the dev server renders its own concise
/// line from these; the CLI prints the full [`Summary`]).
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub pages: usize,
    pub cached: usize,
    /// The directories holding files this build read from outside its own source
    /// trees: a `data/` tree a page loaded, a config a template imported.
    ///
    /// The dev server watches these on top of the four it always watches, so a
    /// file the build demonstrably depends on does not also have to be named in
    /// `serve { include }`. Directories rather than files: a watcher registers
    /// directories, and a file created next to a tracked one has to be seen too.
    pub read: Vec<PathBuf>,
}

/// The bundled documents a build dealt with: the ones it exported, and every
/// one the site asks for.
///
/// The paths are derived from the config and the page set, never from what was
/// written, for the same reason a sidecar's are: a cached bundle produces no
/// artifact, and a keep-set built from what this build wrote would sweep it.
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

/// The build engine. Owns shared project state and drives the pipeline.
pub struct Engine {
    project: Project,
    config: Config,
    /// The resolved theme, when the site names one. Resolved once here rather
    /// than per consumer, since obtaining a package can download it.
    theme: Option<Theme>,
    /// What this binary cannot do that the site asked for, from [`Gate`].
    gaps: Vec<FeatureMissing>,
    /// What the site asked for that its own config withholds, from [`Inert`].
    inert: Vec<SettingInert>,
}

impl Engine {
    pub fn new(config: Config, mode: Mode) -> Result<Self> {
        // Ahead of everything, including the gates: this is the one check whose
        // failure destroys the project rather than the build. Checked here and
        // not at parse because `dist` is not settled until the profile overlay
        // and `--out` have had their say, and every command that writes to
        // `dist` comes through this constructor.
        if let Some((key, source)) = config.paths.swallowed(&config.root) {
            return Err(ConfigError::dist_contains_source(
                &config.paths.dist,
                key,
                &source.display().to_string(),
            )
            .into());
        }
        // A gate can turn a setting off, and a half-applied config is what ships
        // the broken site that guards against.
        let (config, gaps) = Gate::resolve(config);
        let inert = Inert::resolve(&config);
        let theme = Theme::of(&config)?;
        let project = Project::new(&config, mode)?;
        Ok(Self {
            project,
            config,
            theme,
            gaps,
            inert,
        })
    }

    /// Build the site incrementally: reuse cached output for unchanged pages,
    /// recompile the rest in parallel, then copy assets.
    ///
    /// Failure leaves `dist` as the previous build left it, staged assets
    /// included: `deploy` walks the whole directory, so a leftover staging tree
    /// would be uploaded as a duplicate copy of the site's assets.
    pub fn build(&self, ui: &Ui) -> Result<Stats> {
        let built = self.run(ui);
        if built.is_err() {
            let _ = std::fs::remove_dir_all(self.config.asset_staging());
        }
        built
    }

    /// A build, phase by phase. Each phase is a method below; this is the only
    /// place their order is spelled, and every constraint on that order is
    /// documented where it binds.
    fn run(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        let statics = self.stage()?;
        let planned = self.planned("planned build")?;
        let warned = ui.warnings();
        // What this binary cannot do that the site asked for. Reported per build
        // rather than per process: the dev server rebuilds in place, and the
        // warning belongs with the output it explains. `check` stays silent,
        // producing none of what a missing feature would have shaped.
        for gap in &self.gaps {
            ui.warn(*gap);
        }
        // ...and what its own config withholds from it. Same placement, same
        // reason: the setting that will not take effect belongs beside the
        // output that does not show it.
        for inert in &self.inert {
            ui.warn(*inert);
        }
        // A site with no not-found page hands unmatched URLs to whatever its
        // host answers with. `Page::listed` is false for exactly that page, so
        // the whole check is asking whether the page set holds one.
        if !planned.pages.iter().any(|page| !page.listed(&self.config)) {
            ui.warn(crate::error::warning::NotFoundMissing);
        }
        // `before` hooks run after the plan (a hook's output is this build's
        // assets, not this build's content) and ahead of the asset pipeline, so
        // anything they emit into `assets/` (e.g. Tailwind output) is
        // fingerprinted like any asset.
        let hooks = Hooks::new(&self.config);
        hooks.before(ui)?;
        // Built before the asset pipeline, whose `baudelaire:*` JS modules serve
        // the section trees it holds, and after it in [`Pass`], which renders
        // against what the pipeline produced.
        let prepare = self.prepare(&planned.pages)?;
        // Before any compile: a template nothing supplies is one diagnostic
        // naming what asked for it, rather than the compiler's own missing-file
        // report against a generated wrapper, once per page.
        prepare.verify()?;
        #[cfg(feature = "js")]
        let modules = Modules::new(self, &prepare);
        // the asset URL map feeds render-side fingerprint rewriting and folds
        // into the cache fingerprint, so a re-fingerprinted asset invalidates
        // the pages that reference it.
        let assets = Assets::new(
            &self.config,
            self.theme.as_ref(),
            #[cfg(feature = "js")]
            modules.ctx(&planned.pages),
        );
        let processed = assets.process()?;
        let (asset_count, asset_bytes) = (processed.count, processed.bytes);
        debug!(count = asset_count, bytes = asset_bytes, "assets processed");
        let mut emitted = processed.emitted;
        let mut pass = Pass::new(
            self,
            &planned,
            prepare,
            processed.map,
            processed.srcsets,
            // The renderer's own copy: a page is stamped with the digest of the
            // file the pipeline wrote, and the externalized images folded in
            // below carry none.
            emitted.clone(),
        );
        let mut cache = self.cache(&pass, &planned, ui)?;
        // Pass one compiles each page against the backlinks the *last* build
        // recorded, because this build's are not knowable until every page has
        // rendered. `backlinks` below replaces them with the truth and
        // recompiles whatever the guess got wrong.
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
        // Ahead of validation, which weighs each page against what this build
        // actually wrote: a typst-embedded image is the usual way a picture
        // reaches a page, and a budget blind to those would count almost
        // nothing. Nothing here reads the pages, so the order is free.
        let images = self.images(&rendered, &cached, ui)?;
        emitted.absorb(images.emitted());
        self.validate(&rendered, &cached, Some(&emitted), false, ui)?;
        let outputs = Self::outputs(&rendered, &cached);
        Self::write(&outputs)?;
        // Bound from the page *sources*, so a bundle is exported whether its
        // pages recompiled or were served from cache.
        let bundled = self.bundles(&pass, &mut cache, ui)?;
        let artifacts: Vec<&Artifact> = rendered
            .iter()
            .flat_map(|r| &r.artifacts)
            .chain(bundled.drawn.iter())
            .collect();
        Self::artifacts(&artifacts)?;
        // Every page is on disk pointing at the new asset filenames, so the
        // staged asset tree can replace the published one. Before this line a
        // failure leaves `dist` exactly as the previous build left it.
        assets.publish()?;
        cache.save(outputs.iter().map(|out| (out.page, out.html)))?;
        let generated = self.generate(&planned.pages, &outputs, &statics, ui)?;
        self.sweep(&outputs, &statics, &generated, &bundled)?;
        // `after` hooks run once the whole site is on disk (deploy, Pagefind..).
        hooks.after(ui)?;

        // Warnings render as a block ahead of the result line, cargo-style.
        ui.flush();
        let total = rendered.len() + cached.len();
        let page_bytes: u64 = outputs.iter().map(|out| out.html.len() as u64).sum();
        let sidecars = Tally::of(artifacts.iter().copied());
        Summary {
            pages: total,
            cached: cached.len(),
            assets: asset_count + images.count(),
            statics: statics.count,
            generated: generated.count,
            bytes: page_bytes
                + asset_bytes
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
            read: self.outside(cache.read()),
        })
    }

    /// The directories holding `files`, minus anything already inside a source
    /// tree the dev server watches by default.
    ///
    /// Deduped and sorted so a rebuild that read the same files hands back the
    /// same list, which is what lets the watcher tell "nothing new" from "watch
    /// something else" without re-registering on every build.
    fn outside(&self, files: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
        let paths = &self.config.paths;
        let watched = [
            &paths.content,
            &paths.templates,
            &paths.assets,
            &paths.r#static,
        ]
        .map(crate::fs::canonical);
        let mut dirs: Vec<PathBuf> = files
            .filter_map(|file| file.parent().map(std::path::Path::to_path_buf))
            .map(|dir| crate::fs::canonicalize(&dir).unwrap_or(dir))
            .filter(|dir| !watched.iter().any(|root| dir.starts_with(root)))
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
    }

    /// Prepare `dist` and seed it with the static tree.
    ///
    /// A staging tree here is a previous build's failure; it is cleared before
    /// the static copy, which writes into it (see `Static::destination`). Static
    /// goes down first so a generated page or asset at the same output path
    /// overwrites it: static is the lowest-priority source.
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
    fn planned(&self, what: &'static str) -> Result<Planned> {
        let pages = plan(&self.config, &self.project)?;
        debug!(pages = pages.len(), site = self.config.label(), "{what}");
        Ok(Planned {
            pages,
            tracked: self.project.tracked(),
        })
    }

    /// The compile inputs for `pages`: the wrapper text binding each page to its
    /// template, plus the section trees derived from the whole page set.
    ///
    /// Built by the caller rather than by [`Pass`], because a build feeds these
    /// section trees to the JS bundler before the asset pipeline runs, and the
    /// asset pipeline is what a [`Pass`] renders against.
    ///
    /// Writing the tree out is part of building the inputs, not a step a caller
    /// can forget: a template's `#import` of it has to resolve on the very
    /// first compile, including in a fresh checkout where nothing has run yet.
    ///
    /// The TypeScript declarations ride along, for the same reason and with the
    /// opposite deadline: nothing in the build reads them back, but writing
    /// them here is what keeps an editor's types following the config instead
    /// of the last time someone ran `baudelaire mirror`.
    fn prepare<'a>(&'a self, pages: &'a [Page]) -> Result<Prepare<'a>> {
        let prepare = Prepare::new(&self.config, &self.project, self.theme.as_ref(), pages);
        let root = self.project.root();
        for table in prepare.generated() {
            table.write(root)?;
        }
        #[cfg(feature = "js")]
        crate::engine::asset::Declarations::of(&self.config).write(root)?;
        // A content page that imports a table was served the empty one during
        // discovery (the table is derived from the frontmatter that read was
        // producing). Now that the real ones are on disk, the world drops what
        // it cached, so the compile reads what this build wrote.
        self.project.tables_written();
        Ok(prepare)
    }

    /// Load the build cache, keyed on every site-wide input the per-page
    /// dependency tracker cannot see.
    fn cache(&self, pass: &Pass, planned: &Planned, ui: &Ui) -> Result<Cache> {
        // render-side cache inputs: asset renames, the link map, the responsive
        let render = RenderInputs {
            modules: self.project.modules(),
        };
        Cache::load(
            &self.config,
            &render,
            planned.tracked.clone(),
            pass.renderer.maps(),
            self.project.root(),
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
        // Compile only the stale pages (already prepared during the cache
        // split); cached pages keep the HTML they were built with.
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
    /// disagreed with, and warn if the graph never settles.
    ///
    /// The convergence itself is [`Graph::settle`]'s; what is here is the one
    /// part of it that is this engine's, recompiling a page. A repaired page
    /// keeps the sidecars pass one drew for it: those are not redrawn (see
    /// [`Sidecars::none`]) and the build still has to write them.
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
        // A repair is the page's markup again and nothing beside it.
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
                // The entry keeps the dependencies the *full* compile recorded:
                // a repair draws no sidecars, so it never saw the files they read.
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
    /// asset directory: fresh pages carry their refs, cache hits their stored
    /// ones, so the files are present regardless of what recompiled.
    fn images(&self, rendered: &[Rendered], cached: &[Reused], ui: &Ui) -> Result<Images> {
        Images::new(&self.config, self.project.root()).copy(
            rendered
                .iter()
                .flat_map(|r| &r.outputs.images)
                .chain(cached.iter().flat_map(|(_, _, out)| &out.images)),
            ui,
        )
    }

    /// Pair every page, rendered and cache-served alike, with what the render
    /// pass produced for it: the write pass, the blob staging, and the
    /// processors all read this one view.
    fn outputs<'a>(rendered: &'a [Rendered<'a>], cached: &'a [Reused<'a>]) -> Vec<Output<'a>> {
        rendered
            .iter()
            .map(|r| Output {
                page: r.page,
                html: r.html.as_str(),
                fragments: r.outputs.fragments.as_ref(),
                inline: &r.outputs.inline,
            })
            .chain(cached.iter().map(|(page, html, out)| Output {
                page,
                html: html.as_str(),
                fragments: out.fragments.as_ref(),
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
    /// drawn during compile, and the bundled documents.
    ///
    /// Only what was freshly made is here. A cache hit leaves the file the
    /// previous build wrote in place, and the sweep keeps it. Each artifact
    /// carries its own destination, so nothing here has to know which kind it
    /// is holding.
    fn artifacts(artifacts: &[&Artifact]) -> Result<()> {
        artifacts
            .par_iter()
            .try_for_each(|artifact| fs::write_all(&artifact.path, &artifact.bytes))
    }

    /// Export the bundled documents this site asks for: a collection, or the
    /// whole site, as one PDF.
    ///
    /// Unlike a sidecar this belongs to no page, so it cannot ride a page's
    /// cache entry: it carries one of its own, keyed on the module text (which
    /// names every page it binds, in order) and on every file that compile
    /// read.
    #[cfg(feature = "pdf")]
    fn bundles(&self, pass: &Pass<'_>, cache: &mut Cache, ui: &Ui) -> Result<Bundled> {
        let mut bundled = Bundled::default();
        for bundle in Bundle::planned(&self.config, pass.pages) {
            let path = bundle.path(&self.config);
            bundled.paths.push(path.clone());
            let text = bundle.source(&pass.prepare, &self.project)?;
            let fingerprint = Hash::of_bytes(text.as_bytes());
            if cache.reuse_bundle(bundle.id(), &fingerprint, &path) {
                debug!(bundle = bundle.id(), "bundle reused");
                continue;
            }
            let (bytes, deps) = bundle.export(&self.project, &pass.prepare, text)?;
            cache.record_bundle(bundle.id(), fingerprint, &deps);
            ui.page(bundle.id(), PageStatus::Built);
            bundled.drawn.push(Artifact {
                kind: Bundle::KIND,
                path,
                bytes,
            });
        }
        Ok(bundled)
    }

    /// Without the exporter nothing binds one, so there is nothing to write and
    /// nothing to keep.
    ///
    /// It mirrors the `pdf`-on signature exactly so the caller compiles
    /// unchanged in both flavors, which is why it takes a `self` it cannot use
    /// and returns a `Result` it cannot fail.
    #[cfg(not(feature = "pdf"))]
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn bundles(&self, _pass: &Pass<'_>, _cache: &mut Cache, _ui: &Ui) -> Result<Bundled> {
        Ok(Bundled::default())
    }

    /// Run the post-build processors over the finished site.
    fn generate(
        &self,
        pages: &[Page],
        outputs: &[Output],
        statics: &Copied,
        ui: &Ui,
    ) -> Result<Generated> {
        let site = Site {
            config: &self.config,
            pages,
            outputs,
        };
        let mut emitter = Emitter::new(ui, statics.paths.iter().cloned());
        Processors::builtin().run(&site, &mut emitter)?;
        Ok(Generated {
            count: emitter.written(),
            bytes: emitter.bytes(),
            paths: emitter.paths().to_vec(),
        })
    }

    /// Drop orphaned outputs from earlier builds (a removed page or taxonomy
    /// term, a renamed permalink) so `dist` never serves stale files.
    ///
    /// Gated on `prune` so a user managing `dist` by hand can opt out. The
    /// keep-set is every file this build produced: page HTML, static
    /// passthrough, generated files. The asset tree is regenerated wholesale, so
    /// the prune skips it. Runs before `after` hooks, whose outputs (Pagefind..)
    /// are not ours to prune.
    fn sweep(
        &self,
        outputs: &[Output],
        statics: &Copied,
        generated: &Generated,
        bundled: &Bundled,
    ) -> Result<()> {
        if !self.config.prune {
            return Ok(());
        }
        // A sidecar belongs to its page whether or not this build re-drew it,
        // so the keep set is derived from the pages, never from what was written.
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
        )
        .run(&keep)?;
        debug!(pruned, "orphaned outputs removed");
        Ok(())
    }

    /// Compile every page and report diagnostics without writing any output.
    pub fn check(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        let planned = self.planned("planned check")?;
        // Nothing is written, so nothing was fingerprinted: the render pass has
        // no asset renames to apply and no variants to offer.
        let pass = Pass::new(
            self,
            &planned,
            self.prepare(&planned.pages)?,
            AssetMap::new(self.config.asset_prefix()),
            SrcSets::default(),
            Emitted::default(),
        );
        // Prepare + compile every page inline (no cache split, no output).
        let rendered = self.render_pages("checking", pass.pages.iter().collect(), ui, |page| {
            (
                page,
                pass.prepare
                    .input(page)
                    .and_then(|(id, text, fp)| self.compile(page, id, text, fp, &pass)),
            )
        })?;
        // No asset pipeline ran and nothing was written, so there is nothing to
        // weigh a page's loads against: `check` lints, and leaves the budgets to
        // the build that produces the bytes.
        self.validate(&rendered, &[], None, true, ui)?;
        ui.flush();
        ui.done(format_args!(
            "checked {} in {}",
            Count::pages(rendered.len()),
            Dur(timer.elapsed())
        ));
        Ok(Stats {
            pages: rendered.len(),
            cached: 0,
            read: Vec::new(),
        })
    }

    /// Compile a batch of pages in parallel and reduce to their rendered
    /// outputs: a progress bar, a rayon map producing one `(page, outcome)` per
    /// item, then [`Engine::collect`] (status lines + diagnostics). The shared
    /// spine of `build` (over the stale subset) and `check` (every page);
    /// `outcome` supplies the only difference: how one item renders. Validation
    /// is the caller's, since only `build` has cached pages to replay.
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

    /// Report each compile outcome (page status lines and any typst warnings
    /// the compiler raised), returning the rendered pages or, after every
    /// failure has been reported, an error carrying *all* failed pages'
    /// diagnostics (a single failure propagates unchanged).
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
        match BuildFailed::aggregate(errors) {
            Some(e) => Err(e),
            None => Ok(rendered),
        }
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
    /// (link rewriting over the typed DOM) before serialization. Records the
    /// files typst read so the cache can invalidate the page precisely, and
    /// keeps typst's own compile warnings so the build can surface them.
    fn compile<'a>(
        &self,
        page: &'a Page,
        id: FileId,
        text: String,
        fingerprint: Hash,
        pass: &Pass<'_>,
    ) -> Result<Rendered<'a>> {
        // parse only now, for a page actually being (re)compiled.
        let source = Source::new(id, text);
        let world = Tracked::new(self.project.world_for(&source));
        let compiled = typst::compile::<HtmlDocument>(&world);
        // typst warnings (unknown font families, deprecations..) survive a
        // successful compile; bridge them like errors so they render with
        // spans. On failure they are dropped: the errors say more. Typst's
        // blanket "html export is under active development" notice is filtered:
        // HTML is this tool's entire output, so it would fire on every page of
        // every build while telling the author nothing actionable.
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
        let mut rewrite = pass
            .renderer
            .rewrite(&mut doc, page, &self.config, world.inner());
        // An icon that could not be inlined leaves an empty `<svg>` in the DOM,
        // so the page cannot be shipped: fail on the first one.
        // Only the first is reported: the files are independent, and stopping
        // at one error is the contract every other pass has.
        if let Some(invalid) = std::mem::take(&mut rewrite.invalid).into_iter().next() {
            return Err(invalid.into());
        }
        let options = HtmlOptions {
            pretty: self.config.pretty(),
        };
        // Shared by both serializations below, so a failure in either reports
        // with the page's own spans.
        let serialization_failed = |errs| {
            BaudelaireErrorKind::TypstHtml(Self::diagnostics(errs, page, &source, world.inner()))
        };
        let html = typst_html::html(&doc, &options).map_err(&serialization_failed)?;
        // Only the single-file export consumes these, and capturing them costs
        // a second pass over the DOM, so nothing else pays for it.
        let fragments = self
            .config
            .navigation
            .standalone
            .enabled
            .then(|| Fragments::capture(&doc, &options).map_err(&serialization_failed))
            .transpose()?;
        // A sidecar is a second, paged compile of the same page, so only a stale
        // page pays for one. A cache hit keeps the file already in `dist`.
        let (artifacts, drawn) =
            pass.sidecars
                .draw(&self.project, &self.config, &pass.prepare, page)?;
        let mut deps = self.project.dependencies(&world);
        // Inlined icons and embedded assets are read by the render pass, not by
        // typst, so they are absent from the compilation's own accesses. Adding
        // them here puts them under the same content-hash check as every other
        // dependency, instead of needing a mechanism of their own.
        deps.extend(std::mem::take(&mut rewrite.read));
        // A sidecar is a second compile of this page, so the template it imports
        // and everything that template pulls in are inputs to this page's output
        // that its own compile never read. Folded in the same way, and for the
        // same reason: without them an edited card helper changed no hash the
        // cache checks, and every card-bearing page stayed a hit still serving
        // the PNG the old helper drew.
        deps.extend(drawn.files().iter().cloned());
        // Which injected values (`sys.inputs.baudelaire.*`) the page read, across
        // its own source and every `.typ` it depends on: the fine-grained
        // metadata dependency set.
        let mut reads = pass.analyzer.reads(&source, &deps);
        // `datetime.today()` goes through the `World`, which records files
        // only, so nothing else sees it: a page printing the current year stayed
        // a cache hit into the next one. It reads the same clock as
        // `sys.inputs.baudelaire.date`, so record it as that key.
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
            srcsets: rewrite.srcsets,
            assets: rewrite.assets,
            outputs: Outputs {
                images: rewrite.images,
                broken: rewrite.broken,
                anchors: rewrite.anchors,
                deep: rewrite.deep,
                outbound: rewrite.outbound,
                // What this compile *assumed* about the rest of the site, kept
                // so the repair pass can tell it from what the site turned out
                // to be. `None` while the feature is off, which is what leaves
                // such a page immune to the graph entirely.
                backlinks: pass.prepare.digest(page),
                fragments,
                lints: rewrite.lints,
                weight: rewrite.weight,
                inline: rewrite.inline,
            },
            external: rewrite.external,
            artifacts,
            warnings,
        })
    }

    /// Run the post-render validation passes over every page, compiled and
    /// cached alike. Maps each into the decoupled [`Compiled`] view and hands it
    /// to [`Links`]; the pass itself lives in [`check`].
    ///
    /// A cache hit replays the broken links it was built with, because checking
    /// only fresh pages made the gate weaken on rebuild: a second build of a
    /// site with a dangling link reported nothing and `links { strict #true }`
    /// passed.
    ///
    /// `outbound` reaches the network and so is passed only by [`Engine::check`],
    /// which recompiles every page and therefore sees every outbound link. A
    /// build stays offline whatever the config says.
    fn validate(
        &self,
        rendered: &[Rendered],
        cached: &[(&Page, String, Outputs)],
        emitted: Option<&Emitted>,
        outbound: bool,
        ui: &Ui,
    ) -> Result<()> {
        // Cached pages contribute no outbound links: nothing recompiled them, so
        // nothing re-read their anchors. Only `check` asks for them, and it never
        // serves a page from cache.
        let fresh = rendered
            .iter()
            .map(|r| (r.page, r.html.as_str(), &r.outputs, r.external.as_slice()));
        let reused = cached
            .iter()
            .map(|(page, html, outputs)| (*page, html.as_str(), outputs, &[] as &[String]));
        let pages: Vec<CheckedPage> = fresh
            .chain(reused)
            .map(|(page, html, outputs, external)| CheckedPage {
                label: self.relative(page),
                source: &page.source,
                permalink: &page.permalink,
                broken: &outputs.broken,
                external,
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
                generated: matches!(page.data, Data::Generated { .. }),
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
        if self.config.lint.enabled {
            Lints::run(&site, ui)?;
            Budgets::run(&site)?;
        }
        if outbound && self.config.links.external {
            External::run(&site, ui)?;
        }
        Ok(())
    }

    /// Wrap typst source diagnostics with the compiled source so miette renders
    /// spans against exactly what was compiled (the layout-wrapped source, when
    /// a template is bound). Shared by the compile and HTML-emit stages.
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
        )
    }
}

/// The owned inputs one pass over the site borrows: the planned pages and the
/// build's tracked value trees. Separate from [`Pass`] because [`Pass`] holds
/// borrows of both, and no struct can borrow from itself.
struct Planned {
    pages: Vec<Page>,
    /// The injected values whose per-page reads drive fine-grained metadata
    /// invalidation: the analyzer records them from each page's syntax, the
    /// cache re-hashes them to decide reuse. One owned copy backs the cache; the
    /// analyzer borrows another view of the same trees.
    tracked: Vec<(String, Value)>,
}

/// The site values the `baudelaire:*` virtual JS modules serve, built from the
/// same wrapper inputs the templates get rather than recomputed from scratch.
///
/// Owned by the build, because [`JsCtx`] borrows them for as long as the asset
/// pipeline lives.
#[cfg(feature = "js")]
struct Modules {
    /// The codegen `Value` view of the build context. It exists only for these
    /// modules; Typst reads `sys.inputs` from the raw context.
    context: Value,
    /// The section trees keyed by language: one bundle serves the whole site, so
    /// a default-language-only tree left every translation without a nav.
    /// Templates get their own wrapper text and never read this.
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
