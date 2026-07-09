//! Build pipeline: discover → compile → render → write, parallelized via rayon.

mod asset;
mod feed;
mod hook;
mod layout;
mod llms;
mod process;
mod redirect;
mod robots;
mod search;
mod sitemap;
mod text;
mod xml;

use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use typst::syntax::{FileId, Source};
use typst::{World, compile};
use typst_html::{HtmlDocument, HtmlOptions};

use crate::cli::output::{Count, PageStatus, Paths, Report};
use crate::config::Config;
use crate::content::{Page, Pagination, Taxonomy, discover};
use crate::engine::asset::Assets;
use crate::engine::hook::Hooks;
use crate::engine::layout::Layout;
use crate::engine::process::{Emitter, Processors, Site};
use crate::fs;
use crate::error::{BaudelaireErrorKind, Broken, BrokenLinks, Result, TypstSourceDiagnostic};
use crate::graph::{Cache, Deps, Hash};
use crate::render::{AssetMap, Renderer};
use crate::world::{PageWorld, Project, Tracked};
pub use crate::world::Mode;

/// Build statistics returned to callers (the dev server renders its own concise
/// line from these; the CLI prints the full [`Summary`]).
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub pages: usize,
    pub cached: usize,
}

/// The end-of-build summary: one tight result line covering pages (and how many
/// were cached), assets processed, generated files, any warnings, and the output
/// directory. Owns its own rendering so [`Engine::build`] only gathers the
/// numbers. The per-file names stay available at verbose via the emitter notes.
struct Summary<'a> {
    pages: usize,
    cached: usize,
    assets: usize,
    generated: usize,
    warnings: usize,
    dist: &'a Path,
}

impl Summary<'_> {
    fn report(&self, report: &mut Report) -> std::io::Result<()> {
        let mut parts = vec![match self.cached {
            0 => Count::pages(self.pages).to_string(),
            n => format!("{} ({n} cached)", Count::pages(self.pages)),
        }];
        if self.assets > 0 {
            parts.push(Count::assets(self.assets).to_string());
        }
        if self.generated > 0 {
            parts.push(Count::files(self.generated).to_string());
        }
        if self.warnings > 0 {
            parts.push(Count::warnings(self.warnings).to_string());
        }
        report.done(format_args!("{} → {}", parts.join(" · "), Paths(&self.dist.display().to_string())))
    }
}

/// The build engine. Owns shared project state and drives the pipeline.
pub struct Engine {
    project: Project,
    config: Config,
}

impl Engine {
    pub fn new(config: Config, mode: Mode) -> Result<Self> {
        let project = Project::new(&config, mode)?;
        Ok(Self { project, config })
    }

    /// Build the site incrementally: reuse cached output for unchanged pages,
    /// recompile the rest in parallel, then copy assets.
    pub fn build(&self, report: &mut Report) -> Result<Stats> {
        fs::create_dir_all(&self.config.dist)?;
        let pages = self.announce(report, "building")?;
        let warned = report.warnings();

        // `before` hooks run ahead of the asset pipeline, so anything they
        // generate into `assets/` (e.g. a Tailwind stylesheet) is picked up and
        // fingerprinted like a first-class asset.
        let hooks = Hooks::new(&self.config);
        hooks.before(report)?;

        // Assets are processed next: the resulting URL map feeds the render
        // pipeline (fingerprint rewriting) and folds into the cache fingerprint,
        // so a re-fingerprinted asset invalidates the pages that reference it.
        report.info("processing assets")?;
        let (assets, asset_count) = Assets::new(&self.config).process()?;
        let assets_hash = Hash::of(&assets);
        let renderer = Renderer::new(&pages, assets);
        let mut cache = Cache::load(&self.config, self.project.context(), &assets_hash);

        // Split pages into those the cache can serve verbatim and those needing
        // a fresh compile.
        let mut cached: Vec<(&Page, String)> = Vec::new();
        let mut stale: Vec<&Page> = Vec::new();
        for page in &pages {
            match self.fingerprint(page).and_then(|fp| cache.reuse(page, &fp)) {
                Some(html) => cached.push((page, html)),
                None => stale.push(page),
            }
        }

        let outcomes: Vec<(&Page, Result<Rendered>)> = stale
            .par_iter()
            .map(|page| (*page, self.compile(page, &renderer)))
            .collect();
        let rendered = self.collect(outcomes, report)?;
        self.check_links(&rendered, report)?;

        for r in &rendered {
            cache.record(&r.page, r.fingerprint.clone(), &r.html, &r.deps);
            self.write(&r.page.output, &r.html)?;
        }
        for (page, html) in &cached {
            report.page(self.relative(page), PageStatus::Cached)?;
            self.write(&page.output, html)?;
        }

        // Pair every page (freshly rendered and cache-served alike) with its
        // final HTML. Shared by the cache (to stage HTML blobs) and the
        // post-build processors (which derive from page text).
        let outputs: Vec<(&Page, &str)> = rendered
            .iter()
            .map(|r| (&r.page, r.html.as_str()))
            .chain(cached.iter().map(|(page, html)| (*page, html.as_str())))
            .collect();
        cache.save(&outputs)?;
        let generated = {
            let site = Site {
                config: &self.config,
                pages: &pages,
                outputs: &outputs,
            };
            let mut emitter = Emitter::new(report);
            Processors::builtin().run(&site, &mut emitter)?;
            emitter.written()
        };

        // `after` hooks run once the whole site is on disk (deploy scripts,
        // post-processors like Pagefind, …).
        hooks.after(report)?;

        let total = rendered.len() + cached.len();
        Summary {
            pages: total,
            cached: cached.len(),
            assets: asset_count,
            generated,
            warnings: report.warnings() - warned,
            dist: &self.config.dist,
        }
        .report(report)?;
        Ok(Stats { pages: total, cached: cached.len() })
    }

    /// Compile every page and report diagnostics without writing any output.
    pub fn check(&self, report: &mut Report) -> Result<Stats> {
        let pages = self.announce(report, "checking")?;
        let renderer = Renderer::new(&pages, AssetMap::default());
        let outcomes: Vec<(&Page, Result<Rendered>)> = pages
            .par_iter()
            .map(|page| (page, self.compile(page, &renderer)))
            .collect();
        let rendered = self.collect(outcomes, report)?;
        self.check_links(&rendered, report)?;
        report.done(format_args!("checked {}", Count::pages(rendered.len())))?;
        Ok(Stats { pages: rendered.len(), cached: 0 })
    }

    /// Discover eligible pages and print the opening milestone for `action`.
    fn announce(&self, report: &mut Report, action: &str) -> Result<Vec<Page>> {
        let pages = self.pages()?;
        report.milestone(format_args!(
            "{action} {} ({})",
            self.config.label(),
            Count::pages(pages.len())
        ))?;
        report.start();
        Ok(pages)
    }

    /// Report each compile outcome, returning the rendered pages or the first
    /// error after every failure has been reported.
    fn collect(
        &self,
        outcomes: Vec<(&Page, Result<Rendered>)>,
        report: &mut Report,
    ) -> Result<Vec<Rendered>> {
        let mut error = None;
        let mut rendered = Vec::new();
        for (page, outcome) in outcomes {
            match outcome {
                Ok(r) => {
                    report.page(self.relative(page), PageStatus::Built)?;
                    rendered.push(r);
                }
                Err(e) => {
                    report.page(self.relative(page), PageStatus::Failed)?;
                    error.get_or_insert(e);
                }
            }
        }
        match error {
            Some(e) => Err(e),
            None => Ok(rendered),
        }
    }

    /// A page's source path relative to the content root, for display. Handles
    /// both discovered pages (content-relative) and generated pages (whose
    /// synthetic sources are canonical-absolute).
    fn relative(&self, page: &Page) -> String {
        let canonical = self.config.content.canonicalize();
        let canonical = canonical.as_deref().unwrap_or(&self.config.content);
        page.source
            .strip_prefix(canonical)
            .or_else(|_| page.source.strip_prefix(&self.config.content))
            .unwrap_or(&page.source)
            .display()
            .to_string()
    }

    /// Discover and filter pages eligible for build, plus generated taxonomy
    /// and paginated index pages built from them.
    fn pages(&self) -> Result<Vec<Page>> {
        let collections = discover(&self.config)?;
        let mut pages: Vec<Page> = collections
            .iter()
            .flat_map(|c| c.pages.iter())
            .filter(|p| !p.skipped(self.config.draft.build, self.config.future))
            .cloned()
            .collect();
        pages.extend(Taxonomy::pages(&self.config, &pages));
        pages.extend(Pagination::pages(&self.config, &collections));
        Ok(pages)
    }

    /// Compile a single page to rendered HTML, applying render post-processing
    /// (link rewriting over the typed DOM) before serialization. Records the
    /// files typst read so the cache can invalidate the page precisely.
    fn compile(&self, page: &Page, renderer: &Renderer) -> Result<Rendered> {
        let source = self.source_for(page)?;
        let fingerprint = Hash::of_bytes(source.text().as_bytes());
        let world = Tracked::new(self.project.world_for(&source));
        let mut doc = compile::<HtmlDocument>(&world).output.map_err(|errs| {
            BaudelaireErrorKind::TypstCompile(self.diagnostics(errs, page, &source, world.inner()))
        })?;
        let broken = renderer.rewrite(&mut doc, page, &self.config);
        let html = typst_html::html(&doc, &self.html_options()).map_err(|errs| {
            BaudelaireErrorKind::TypstHtml(self.diagnostics(errs, page, &source, world.inner()))
        })?;
        Ok(Rendered {
            page: page.clone(),
            fingerprint,
            html,
            deps: self.project.dependencies(&world),
            broken,
        })
    }

    /// Content fingerprint of a page: a hash of the exact text typst compiles
    /// (real file body or synthetic layout/generated source). Unlike hashing the
    /// source *path*, this fingerprints generated pages too — their sources never
    /// touch disk. `None` only if the synthetic source can't be virtualized.
    fn fingerprint(&self, page: &Page) -> Option<Hash> {
        Some(Hash::of_bytes(self.source_for(page).ok()?.text().as_bytes()))
    }

    /// Report broken internal links found while compiling `rendered`. Under
    /// `strict_links` any broken link fails the build; otherwise each is a
    /// warning. Only freshly compiled pages are checked — cached pages kept
    /// their links from when they were built.
    fn check_links(&self, rendered: &[Rendered], report: &mut Report) -> Result<()> {
        let mut broken = Vec::new();
        for r in rendered {
            for target in &r.broken {
                broken.push(Broken::new(
                    self.relative(&r.page),
                    target.clone(),
                    &r.page.source,
                ));
            }
        }
        if broken.is_empty() {
            return Ok(());
        }
        if self.config.links.strict {
            return Err(BrokenLinks::new(broken).into());
        }
        report.warn(format_args!(
            "{} broken internal link{}",
            broken.len(),
            if broken.len() == 1 { "" } else { "s" }
        ))?;
        for b in &broken {
            report.item(format_args!("`{}` in {}", b.target, b.page))?;
        }
        Ok(())
    }

    /// The source typst compiles for a page: its body, or — when the page
    /// selects a layout — a synthetic module that binds the body to the
    /// template.
    fn source_for(&self, page: &Page) -> Result<Source> {
        let rooted = self.project.virtualize(&page.source)?;
        let id = FileId::new(rooted);
        let text = match &page.template {
            Some(template) => {
                Layout::new(&self.config.templates, template, &page.data, &page.body).to_string()
            }
            None => page.body.clone(),
        };
        Ok(Source::new(id, text))
    }

    fn html_options(&self) -> HtmlOptions {
        HtmlOptions {
            pretty: self.config.html.pretty,
        }
    }

    /// Wrap typst source diagnostics with the compiled source so miette renders
    /// spans against exactly what was compiled (the layout-wrapped source, when
    /// a template is bound). Shared by the compile and HTML-emit stages.
    fn diagnostics(
        &self,
        errs: typst::ecow::EcoVec<typst::diag::SourceDiagnostic>,
        page: &Page,
        source: &Source,
        world: &PageWorld,
    ) -> Vec<TypstSourceDiagnostic> {
        let world: Arc<dyn World + Send + Sync> = Arc::new(world.clone());
        errs.into_iter()
            .map(|e| {
                // Render each diagnostic against the file its span belongs to, so
                // an error reaching into a bound template or shared module lands
                // in the right source instead of overrunning the page text.
                let file = e.span.id();
                let named = file
                    .and_then(|id| world.source(id).ok().map(|src| (id, src)))
                    .map(|(id, src)| {
                        let name = id.vpath().get_without_slash().to_string();
                        miette::NamedSource::new(name, src.text().to_owned())
                    })
                    .unwrap_or_else(|| {
                        miette::NamedSource::new(
                            page.source.display().to_string(),
                            source.text().to_owned(),
                        )
                    });
                TypstSourceDiagnostic::new(e, named, file, world.clone())
            })
            .collect()
    }

    /// Write a page's HTML, creating parent directories. Errors propagate with
    /// the offending path (previously they were silently discarded).
    fn write(&self, out: &Path, html: &str) -> Result<()> {
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out, html)
    }
}

/// A compiled page ready to write, with the files its compilation depended on
/// and the raw targets of any broken internal links it contained.
struct Rendered {
    page: Page,
    fingerprint: Hash,
    html: String,
    deps: Deps,
    broken: Vec<String>,
}

