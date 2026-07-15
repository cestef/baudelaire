//! Build pipeline: discover → compile → render → write, parallelized via rayon.

mod asset;
mod exif;
mod feed;
mod hook;
mod layout;
mod llms;
mod process;
mod prune;
mod redirect;
mod robots;
mod search;
mod sitemap;
mod standard;
mod statics;
pub mod text;
mod xml;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rayon::prelude::*;
use tracing::debug;
use typst::compile;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst_html::{HtmlDocument, HtmlOptions};

use crate::config::Config;
use crate::content::{Data, Page, Section, plan};
use crate::engine::asset::Assets;
use crate::engine::hook::Hooks;
use crate::engine::layout::{Bind, Body, Layout};
use crate::engine::process::{Emitter, Processors, Site};
use crate::engine::statics::Static;
use crate::error::{
    BaudelaireErrorKind, Broken, BrokenLinks, BuildFailed, Result, TypstSourceDiagnostic,
};
use crate::fs;
use crate::graph::{Cache, Deps, Fingerprint, Hash, RenderInputs};
use crate::render::{AssetMap, Renderer};
use crate::ui::{Bytes, Count, Dur, PageStatus, Paths, Timer, Ui};
pub use crate::world::Mode;
use crate::world::{PageWorld, Project, Tracked};

/// Build statistics returned to callers (the dev server renders its own concise
/// line from these; the CLI prints the full [`Summary`]).
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub pages: usize,
    pub cached: usize,
}

/// A page reduced to what the cache check needs — its `FileId`, the exact text
/// typst will compile, and that text's fingerprint — before the costly parse
/// into a `Source` (deferred to [`Engine::compile`], run only for stale pages).
type Prepared = (FileId, String, Hash);

/// The end-of-build summary: one tight result line covering pages (and how many
/// were cached), assets processed, generated files, any warnings, the output
/// directory, and elapsed time. Owns its own rendering so [`Engine::build`]
/// only gathers the numbers.
struct Summary<'a> {
    pages: usize,
    cached: usize,
    assets: usize,
    statics: usize,
    generated: usize,
    bytes: u64,
    warnings: usize,
    dist: &'a Path,
    elapsed: Duration,
}

impl Summary<'_> {
    fn report(&self, ui: &Ui) {
        let mut parts = vec![match self.cached {
            0 => Count::pages(self.pages).to_string(),
            n => format!("{} ({n} cached)", Count::pages(self.pages)),
        }];
        if self.assets > 0 {
            parts.push(Count::assets(self.assets).to_string());
        }
        if self.statics > 0 {
            parts.push(Count::statics(self.statics).to_string());
        }
        if self.generated > 0 {
            parts.push(Count::files(self.generated).to_string());
        }
        if self.bytes > 0 {
            parts.push(Bytes(self.bytes).to_string());
        }
        if self.warnings > 0 {
            parts.push(Count::warnings(self.warnings).to_string());
        }
        ui.done(format_args!(
            "built {} → {} in {}",
            parts.join(" · "),
            Paths(&self.dist.display().to_string()),
            Dur(self.elapsed)
        ));
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
    pub fn build(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        fs::create_dir_all(&self.config.dist)?;
        // Copy the static tree first, so a generated page or asset at the same
        // output path overwrites it — static is the lowest-priority source.
        let statics = Static::new(&self.config).copy()?;
        debug!(
            count = statics.count,
            bytes = statics.bytes,
            "static copied"
        );
        let pages = plan(&self.config, &self.project)?;
        debug!(
            pages = pages.len(),
            site = self.config.label(),
            "planned build"
        );
        let warned = ui.warnings();

        // `before` hooks run ahead of the asset pipeline so anything they emit
        // into `assets/` (e.g. Tailwind output) is fingerprinted like any asset.
        let hooks = Hooks::new(&self.config);
        hooks.before(ui)?;

        // Build the section tree and the `sys.inputs.baudelaire` value once: both
        // feed templates (`page.sections`, `sys.inputs`) AND the `baudelaire:*`
        // JS modules, which reuse these rather than recompute them.
        let sections = self.sections(&pages);
        let context = crate::codegen::Value::from(self.project.context());

        // the asset URL map feeds render-side fingerprint rewriting and folds
        // into the cache fingerprint, so a re-fingerprinted asset invalidates
        // the pages that reference it.
        let assets = Assets::new(&self.config, &pages, &context, &sections).process()?;
        let asset_count = assets.count;
        let asset_bytes = assets.bytes;
        debug!(count = asset_count, bytes = asset_bytes, "assets processed");
        let renderer = Renderer::new(&pages, assets.map, self.project.root());
        // render-side cache inputs: asset renames, the link map, and — only
        // when pages inline asset bytes — the embedded contents (the per-page
        // dependency tracker can't see them, since typst never reads them).
        let render = RenderInputs {
            assets: renderer.assets().fingerprint(),
            links: renderer.links(),
            // hash the *processed* asset tree — what Embed actually inlines: a
            // bundle's bytes can change through imports outside the source
            // assets dir (../lib, node_modules), which a source-dir hash misses.
            embeds: self
                .config
                .html
                .embed
                .then(|| Hash::of_dir(&self.config.asset_dist())),
        };
        let mut cache = Cache::load(&self.config, self.project.context(), &render, ui)?;

        // split cache hits from stale pages. `prepare` produces each page's
        // text + fingerprint without parsing it — the parse into a typst
        // `Source` is deferred to `compile`, so a hit never pays to parse a
        // page it won't render.
        // the section tree as wrapper text: exposed to every template as
        // `page.sections`, and part of each page's wrapper → a title/url change
        // refingerprints every page that embeds the nav (correct: the sidebar
        // renders on all of them).
        let sections = crate::codegen::Typst(&sections).to_string();

        let mut cached: Vec<(&Page, String)> = Vec::new();
        let mut stale: Vec<(&Page, Result<Prepared>)> = Vec::new();
        for page in &pages {
            match self.prepare(page, &sections) {
                Ok((id, text, fingerprint)) => match cache.reuse(page, &fingerprint) {
                    Some(html) => cached.push((page, html)),
                    None => stale.push((page, Ok((id, text, fingerprint)))),
                },
                Err(e) => stale.push((page, Err(e))),
            }
        }
        debug!(stale = stale.len(), reused = cached.len(), "cache split");

        let progress = ui.progress("compiling", stale.len());
        let outcomes: Vec<(&Page, Result<Rendered>)> = stale
            .into_par_iter()
            .map(|(page, prepared)| {
                let outcome =
                    prepared.and_then(|(id, text, fp)| self.compile(page, id, text, fp, &renderer));
                progress.tick(self.relative(page));
                (page, outcome)
            })
            .collect();
        progress.finish();
        let rendered = self.collect(outcomes, ui)?;
        self.check_links(&rendered, ui)?;

        for r in &rendered {
            cache.record(r.page, r.fingerprint, &r.html, &r.deps);
        }
        for (page, _) in &cached {
            ui.page(self.relative(page), PageStatus::Cached);
        }
        // pair every page (rendered and cache-served alike) with its final
        // HTML once: write pass, blob staging, and processors share this view.
        let outputs: Vec<(&Page, &str)> = rendered
            .iter()
            .map(|r| (r.page, r.html.as_str()))
            .chain(cached.iter().map(|(page, html)| (*page, html.as_str())))
            .collect();
        // write page HTML in parallel — independent files, no shared state.
        outputs
            .par_iter()
            .try_for_each(|(page, html)| fs::write_all(&page.output, html))?;
        cache.save(&outputs)?;
        let (generated, generated_bytes, generated_paths) = {
            let site = Site {
                config: &self.config,
                pages: &pages,
                outputs: &outputs,
            };
            let mut emitter = Emitter::new(ui);
            Processors::builtin().run(&site, &mut emitter)?;
            (emitter.written(), emitter.bytes(), emitter.paths().to_vec())
        };
        let page_bytes: u64 = outputs.iter().map(|(_, html)| html.len() as u64).sum();

        // Drop orphaned outputs from earlier builds (a removed page or taxonomy
        // term, a renamed permalink) so `dist` never serves stale files. Gated
        // on `clean` so a user managing `dist` by hand can opt out. The keep-set
        // is every file this build produced — page HTML, static passthrough,
        // generated files; the asset tree the pipeline already regenerates
        // wholesale, so the prune skips it. Runs before `after` hooks, whose
        // outputs (Pagefind..) aren't ours to prune.
        if self.config.clean {
            let keep: Vec<PathBuf> = outputs
                .iter()
                .map(|(page, _)| page.output.clone())
                .chain(statics.paths.iter().cloned())
                .chain(generated_paths)
                .collect();
            let pruned = prune::Prune::new(
                &self.config.dist,
                &self.config.asset_dist(),
                &self.config.cache.dir,
            )
            .run(&keep)?;
            debug!(pruned, "orphaned outputs removed");
        }

        // `after` hooks run once the whole site is on disk (deploy, Pagefind..).
        hooks.after(ui)?;

        // Warnings render as a block ahead of the result line, cargo-style.
        ui.flush();
        let total = rendered.len() + cached.len();
        Summary {
            pages: total,
            cached: cached.len(),
            assets: asset_count,
            statics: statics.count,
            generated,
            bytes: page_bytes + asset_bytes + generated_bytes + statics.bytes,
            warnings: ui.warnings() - warned,
            dist: &self.config.dist,
            elapsed: timer.elapsed(),
        }
        .report(ui);
        Ok(Stats {
            pages: total,
            cached: cached.len(),
        })
    }

    /// Compile every page and report diagnostics without writing any output.
    pub fn check(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        let pages = plan(&self.config, &self.project)?;
        debug!(
            pages = pages.len(),
            site = self.config.label(),
            "planned check"
        );
        let renderer = Renderer::new(&pages, AssetMap::default(), self.project.root());
        let sections = crate::codegen::Typst(&self.sections(&pages)).to_string();
        let progress = ui.progress("checking", pages.len());
        let outcomes: Vec<(&Page, Result<Rendered>)> = pages
            .par_iter()
            .map(|page| {
                let outcome = self
                    .prepare(page, &sections)
                    .and_then(|(id, text, fp)| self.compile(page, id, text, fp, &renderer));
                progress.tick(self.relative(page));
                (page, outcome)
            })
            .collect();
        progress.finish();
        let rendered = self.collect(outcomes, ui)?;
        self.check_links(&rendered, ui)?;
        ui.flush();
        ui.done(format_args!(
            "checked {} in {}",
            Count::pages(rendered.len()),
            Dur(timer.elapsed())
        ));
        Ok(Stats {
            pages: rendered.len(),
            cached: 0,
        })
    }

    /// Report each compile outcome — page status lines and any typst warnings
    /// the compiler raised — returning the rendered pages or, after every
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
        let canonical = fs::canonical(&self.config.content);
        page.source
            .strip_prefix(canonical)
            .or_else(|_| page.source.strip_prefix(&self.config.content))
            .unwrap_or(&page.source)
            .display()
            .to_string()
    }

    /// The compile input for a page: its (possibly synthetic) source and its
    /// content fingerprint — a hash of the exact text typst compiles. A real
    /// page's body reaches the compiler through `#include` (a tracked file
    /// read, so the dependency cache covers its edits); only generated
    /// listings, which have no file, inline their body — and only their
    /// wrapper text needs fingerprinting. Built once and shared by the cache
    /// check and the compile.
    fn prepare(&self, page: &Page, sections: &str) -> Result<Prepared> {
        let rooted = self.project.virtualize(&page.source)?;
        let Some(template) = &page.template else {
            let text = page.body.clone();
            let fingerprint = Hash::of_bytes(text.as_bytes());
            return Ok((FileId::new(rooted), text, fingerprint));
        };
        let taxonomies = crate::codegen::Typst(&page.taxonomies()).to_string();
        // prev/next sibling links, exposed to the template as `page.nav`. Part of
        // the wrapper text, so a neighbour's addition, removal, or retitling
        // refingerprints this page and rebuilds it — the cache stays correct.
        let nav = crate::codegen::Typst(&Self::nav(&page.siblings)).to_string();
        let vpath = Self::rooted_str(&rooted);
        let (id, bind, body) = match &page.data {
            Data::Export => (Self::wrapper(&rooted), Bind::Import, Body::Include),
            Data::Empty => (Self::wrapper(&rooted), Bind::Literal("(:)"), Body::Include),
            Data::Generated(dict) => (
                FileId::new(rooted.clone()),
                Bind::Literal(dict),
                Body::Inline(&page.body),
            ),
        };
        let context = layout::Context {
            data: bind,
            taxonomies: &taxonomies,
            nav: &nav,
            sections,
        };
        let text = Layout::new(&self.config.templates, template, &vpath, context, body).to_string();
        // hash the exact text typst compiles; the parse into a `Source` is
        // deferred to `compile`, run only for stale pages.
        let fingerprint = Hash::of_bytes(text.as_bytes());
        Ok((id, text, fingerprint))
    }

    /// The site's [`Section`] tree as a value: exposed to every template as
    /// `page.sections` (the single source a site nav is built from, so it can't
    /// drift from the pages) and reused by the `baudelaire:sections` JS module.
    /// Each node is `(id, pages: ((url, title), ..), children: (..))`, one per
    /// content directory; generated listings are excluded.
    fn sections(&self, pages: &[Page]) -> crate::codegen::Value {
        crate::codegen::Value::array(
            Section::tree(pages, &self.config)
                .iter()
                .map(Section::value),
        )
    }

    /// The prev/next sibling links as a typst dict value:
    /// `(prev: (url: .., title: ..), next: none)`. Each link is a dict or `none`,
    /// so a template reads `page.nav.prev.url` / `page.nav.next` uniformly.
    fn nav(siblings: &crate::content::Siblings) -> crate::codegen::Value {
        use crate::codegen::Value;
        let link = |s: &Option<crate::content::Sibling>| match s {
            Some(s) => Value::dict([("url", Value::str(&s.url)), ("title", Value::str(&s.title))]),
            None => Value::None,
        };
        Value::dict([
            ("prev", link(&siblings.prev)),
            ("next", link(&siblings.next)),
        ])
    }

    /// A page's project-root-absolute virtual path (`/content/posts/a.typ`) —
    /// what the wrapper's `#import`/`#include` literals resolve against.
    fn rooted_str(rooted: &RootedPath) -> String {
        format!("/{}", rooted.vpath().get_without_slash())
    }

    /// The synthetic wrapper's file id: a sibling of the page (so relative
    /// template imports resolve the same way), but distinct from it, so the
    /// wrapper can `#include` the real file without shadowing it as `main`.
    fn wrapper(rooted: &RootedPath) -> FileId {
        let name = format!("{}@layout", rooted.vpath().get_without_slash());
        let vpath = VirtualPath::new(&name)
            .expect("a page vpath with a suffix stays a valid relative vpath");
        FileId::new(RootedPath::new(VirtualRoot::Project, vpath))
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
        renderer: &Renderer,
    ) -> Result<Rendered<'a>> {
        // parse only now, for a page actually being (re)compiled.
        let source = Source::new(id, text);
        let world = Tracked::new(self.project.world_for(&source));
        let compiled = compile::<HtmlDocument>(&world);
        // typst warnings (unknown font families, deprecations..) survive a
        // successful compile; bridge them like errors so they render with
        // spans. On failure they are dropped — the errors say more. Typst's
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
        let warnings = self.diagnostics(warnings, page, &source, world.inner());
        let mut doc = compiled.output.map_err(|errs| {
            BaudelaireErrorKind::TypstCompile(self.diagnostics(errs, page, &source, world.inner()))
        })?;
        let broken = renderer.rewrite(&mut doc, page, &self.config);
        let html = typst_html::html(&doc, &self.html_options()).map_err(|errs| {
            BaudelaireErrorKind::TypstHtml(self.diagnostics(errs, page, &source, world.inner()))
        })?;
        Ok(Rendered {
            page,
            fingerprint,
            html,
            deps: self.project.dependencies(&world),
            broken,
            warnings,
        })
    }

    /// Report broken internal links found while compiling `rendered`. Under
    /// `strict_links` any broken link fails the build; otherwise the same
    /// diagnostic — spans, offending pages and all — is collected as a
    /// warning. Only freshly compiled pages are checked — cached pages kept
    /// their links from when they were built.
    fn check_links(&self, rendered: &[Rendered], ui: &Ui) -> Result<()> {
        let mut broken = Vec::new();
        for r in rendered {
            for target in &r.broken {
                broken.push(Broken::new(
                    self.relative(r.page),
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
        ui.warn(BrokenLinks::warning(broken));
        Ok(())
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
        TypstSourceDiagnostic::bridge(
            errs,
            (&page.source.display().to_string(), source.text()),
            Arc::new(world.clone()),
        )
    }
}

/// A compiled page ready to write, with the files its compilation depended on,
/// the raw targets of any broken internal links it contained, and the warnings
/// typst raised while compiling it.
struct Rendered<'a> {
    page: &'a Page,
    fingerprint: Hash,
    html: String,
    deps: Deps,
    broken: Vec<String>,
    warnings: Vec<TypstSourceDiagnostic>,
}
