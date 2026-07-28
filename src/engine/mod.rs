//! Build pipeline: discover -> compile -> render -> write, parallelized via rayon.

mod asset;
mod check;
mod compile;
mod emit;
mod hook;
mod layers;
mod prune;
mod statics;
mod summary;
pub mod text;

use std::path::PathBuf;
use std::sync::Arc;

use rayon::prelude::*;
use tracing::debug;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst_html::{HtmlDocument, HtmlOptions};

use crate::config::Config;
use crate::content::{Data, Page, Section, plan};
use crate::engine::asset::Assets;
#[cfg(feature = "js")]
use crate::engine::asset::JsCtx;
use crate::engine::check::External;
use crate::engine::check::{CheckedPage, Compiled, Links};
#[cfg(feature = "cards")]
use crate::engine::compile::card::Card;
use crate::engine::compile::image::Images;
use crate::engine::compile::layout::{Bind, Body, Layout};
use crate::engine::emit::{Emitter, Output, Processors, Site};
use crate::engine::hook::Hooks;
use crate::engine::statics::{Copied, Static};
use crate::engine::summary::Summary;
use crate::error::{BaudelaireErrorKind, BuildFailed, Result, TypstSourceDiagnostic};
use crate::fs;
use crate::graph::{Analyzer, Cache, Deps, Fingerprint, Hash, Outputs, Reads, RenderInputs, Root};
use crate::render::{AssetMap, Fragments, Renderer, SrcSets};
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
}

/// What the card renderer produced this build, for the summary. A cache hit
/// re-renders nothing, so these count only fresh cards; the files from earlier
/// builds are still in `dist` and still kept.
#[derive(Default)]
struct Cards {
    count: usize,
    bytes: u64,
}

impl Cards {
    #[cfg(feature = "cards")]
    fn of(rendered: &[Rendered]) -> Self {
        rendered
            .iter()
            .filter_map(|r| r.card.as_ref())
            .fold(Self::default(), |acc, card| Self {
                count: acc.count + 1,
                bytes: acc.bytes + card.len() as u64,
            })
    }

    /// Without the rasterizer nothing renders one, so there is nothing to count.
    #[cfg(not(feature = "cards"))]
    fn of(_rendered: &[Rendered]) -> Self {
        Self::default()
    }
}

/// What the post-build processors emitted, for the summary and the prune.
struct Generated {
    count: usize,
    bytes: u64,
    paths: Vec<PathBuf>,
}

/// A page reduced to what the cache check needs: its `FileId`, the exact text
/// typst will compile, and that text's fingerprint, before the costly parse
/// into a `Source` (deferred to [`Engine::compile`], run only for stale pages).
type Prepared = (FileId, String, Hash);

/// Per-language section-tree wrapper text, keyed by language code and picked by
/// a page's language for its `page.sections`.
struct Trees(std::collections::BTreeMap<String, String>);

impl Trees {
    /// This language's section tree as wrapper text (empty when none was built).
    fn get(&self, lang: &str) -> &str {
        self.0.get(lang).map_or("", String::as_str)
    }
}

/// The build engine. Owns shared project state and drives the pipeline.
pub struct Engine {
    project: Project,
    config: Config,
    /// The resolved theme, when the site names one. Resolved once here rather
    /// than per consumer, since obtaining a package can download it.
    theme: Option<Theme>,
}

impl Engine {
    pub fn new(config: Config, mode: Mode) -> Result<Self> {
        let theme = config
            .theme
            .as_deref()
            .map(|spec| Theme::resolve(spec, &config.root))
            .transpose()?;
        let project = Project::new(&config, mode)?;
        Ok(Self {
            project,
            config,
            theme,
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

    fn run(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        fs::create_dir_all(&self.config.dist)?;
        // A staging tree here is a previous build's failure; clear it before the
        // static copy, which writes into it (see `Static::destination`).
        let _ = std::fs::remove_dir_all(self.config.asset_staging());
        // Copy the static tree first, so a generated page or asset at the same
        // output path overwrites it; static is the lowest-priority source.
        let statics = Static::new(&self.config, self.theme.as_ref()).copy()?;
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

        // Build the section trees and the `sys.inputs.baudelaire` value once: both
        // feed templates (`page.sections`, `sys.inputs`) AND the `baudelaire:*`
        // JS modules, which reuse these rather than recompute them.
        let trees = self.trees(&pages);
        // Keyed by language: one bundle serves the whole site, so a
        // default-language-only tree left every translation without a nav.
        // Only the JS modules read this; templates get their own from `trees`.
        #[cfg(feature = "js")]
        let sections = crate::codegen::Value::dict(
            self.config
                .langs()
                .into_iter()
                .map(|lang| (lang.to_owned(), self.sections(&pages, lang))),
        );
        // The codegen `Value` view of the build context exists only to feed the
        // `baudelaire:*` JS modules; Typst reads `sys.inputs` from the raw context.
        #[cfg(feature = "js")]
        let context = crate::codegen::Value::from(self.project.context());

        // the asset URL map feeds render-side fingerprint rewriting and folds
        // into the cache fingerprint, so a re-fingerprinted asset invalidates
        // the pages that reference it.
        let assets = Assets::new(
            &self.config,
            self.theme.as_ref(),
            #[cfg(feature = "js")]
            JsCtx {
                pages: &pages,
                context: &context,
                sections: &sections,
            },
        );
        let processed = assets.process()?;
        let asset_count = processed.count;
        let asset_bytes = processed.bytes;
        debug!(count = asset_count, bytes = asset_bytes, "assets processed");
        let renderer = Renderer::new(
            &pages,
            processed.map,
            processed.srcsets,
            self.project.root(),
        );
        // render-side cache inputs: asset renames, the link map, the responsive
        // variant manifest, and, only when pages inline asset bytes, the embedded
        // contents (the per-page dependency tracker can't see them, since typst
        // never reads them).
        let render = RenderInputs {
            assets: renderer.assets().fingerprint(),
            links: renderer.links(),
            srcsets: renderer.srcsets(),
            // hash the *processed* asset tree, what Embed actually inlines: a
            // bundle's bytes can change through imports outside the source
            // assets dir (../lib, node_modules), which a source-dir hash misses.
            embeds: self
                .config
                .html
                .embed
                .then(|| Hash::of_dir(&self.config.asset_staging())),
            // `None` when cards are off *or* the template is missing; the
            // second case fails the build on the first page it renders, so it
            // never reaches a silent cache hit.
            cards: self
                .config
                .cards
                .active()
                .then(|| Hash::of_file(&self.card_template()))
                .flatten(),
            modules: self.project.modules(),
        };
        // The injected values whose per-page reads drive fine-grained metadata
        // invalidation: the analyzer records them from each page's syntax, the
        // cache re-hashes them to decide reuse. One owned copy backs the cache;
        // the analyzer borrows another view of the same trees.
        let tracked = self.project.tracked();
        let roots: Vec<Root> = tracked
            .iter()
            .map(|(base, tree)| Root { base, tree })
            .collect();
        let analyzer = Analyzer::new(roots, &self.project);
        let mut cache = Cache::load(
            &self.config,
            &render,
            tracked.clone(),
            self.project.root(),
            ui,
        )?;

        // split cache hits from stale pages. `prepare` produces each page's
        // text + fingerprint without parsing it; the parse into a typst
        // `Source` is deferred to `compile`, so a hit never pays to parse a
        // page it won't render.
        let mut cached: Vec<(&Page, String, Outputs)> = Vec::new();
        let mut stale: Vec<(&Page, Result<Prepared>)> = Vec::new();
        for page in &pages {
            match self.prepare(page, &trees) {
                Ok((id, text, fingerprint)) => match cache.reuse(page, &fingerprint) {
                    Some((html, outputs)) => cached.push((page, html, outputs)),
                    None => stale.push((page, Ok((id, text, fingerprint)))),
                },
                Err(e) => stale.push((page, Err(e))),
            }
        }
        debug!(stale = stale.len(), reused = cached.len(), "cache split");

        // Compile only the stale pages (already prepared during the cache
        // split); cached pages keep the HTML they were built with.
        let rendered = self.render_pages("compiling", stale, ui, |(page, prepared)| {
            (
                page,
                prepared.and_then(|(id, text, fp)| {
                    self.compile(page, id, text, fp, &renderer, &analyzer)
                }),
            )
        })?;

        for r in &rendered {
            cache.record(
                r.page,
                r.fingerprint,
                &r.html,
                &r.deps,
                &r.reads,
                &r.outputs,
            );
        }
        for (page, _, _) in &cached {
            ui.page(self.relative(page), PageStatus::Cached);
        }
        self.validate(&rendered, &cached, false, ui)?;
        // Copy every page's externalized images into the (freshly regenerated)
        // asset directory: fresh pages carry their refs, cache hits their stored
        // ones, so the files are present regardless of what recompiled.
        let images = Images::new(&self.config, self.project.root()).copy(
            rendered
                .iter()
                .flat_map(|r| &r.outputs.images)
                .chain(cached.iter().flat_map(|(_, _, out)| &out.images)),
            ui,
        )?;
        // pair every page (rendered and cache-served alike) with what the render
        // pass produced for it once: write pass, blob staging, and processors
        // share this view.
        let outputs: Vec<Output> = rendered
            .iter()
            .map(|r| Output {
                page: r.page,
                html: r.html.as_str(),
                fragments: r.outputs.fragments.as_ref(),
            })
            .chain(cached.iter().map(|(page, html, out)| Output {
                page,
                html: html.as_str(),
                fragments: out.fragments.as_ref(),
            }))
            .collect();
        // write page HTML in parallel: independent files, no shared state.
        outputs
            .par_iter()
            .try_for_each(|out| fs::write_all(&out.page.output, out.html))?;
        // Cards were rendered during compile, so only stale pages produced one;
        // a cache hit leaves the file the previous build wrote in place, and the
        // sweep below keeps it.
        #[cfg(feature = "cards")]
        rendered.par_iter().try_for_each(|r| match &r.card {
            Some(png) => fs::write_all(
                self.config.cards.path(&self.config.dist, &r.page.permalink),
                png,
            ),
            None => Ok(()),
        })?;
        // Every page is on disk pointing at the new asset filenames, so the
        // staged asset tree can replace the published one. Before this line a
        // failure leaves `dist` exactly as the previous build left it.
        assets.publish()?;
        cache.save(outputs.iter().map(|out| (out.page, out.html)))?;
        let generated = self.generate(&pages, &outputs, &statics, ui)?;
        self.sweep(&outputs, &statics, &generated)?;

        // `after` hooks run once the whole site is on disk (deploy, Pagefind..).
        hooks.after(ui)?;

        // Warnings render as a block ahead of the result line, cargo-style.
        ui.flush();
        let total = rendered.len() + cached.len();
        let page_bytes: u64 = outputs.iter().map(|out| out.html.len() as u64).sum();
        let cards = Cards::of(&rendered);
        Summary {
            pages: total,
            cached: cached.len(),
            assets: asset_count + images.count(),
            statics: statics.count,
            generated: generated.count,
            cards: cards.count,
            bytes: page_bytes
                + asset_bytes
                + images.bytes()
                + generated.bytes
                + statics.bytes
                + cards.bytes,
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

    /// The import root a page's layout is loaded from.
    ///
    /// The project's own template directory, expressed relative to the root
    /// because a typst import is root-absolute in the compiler's terms, not the
    /// config's. A template the project does not have falls back to the theme's
    /// package, which the compiler resolves by spec rather than by path.
    fn template_root(&self, template: &str) -> String {
        let project = self
            .config
            .templates
            .strip_prefix(self.project.root())
            .unwrap_or(&self.config.templates);
        match &self.theme {
            Some(theme)
                if !self.config.templates.join(template).is_file()
                    && theme.has_template(template) =>
            {
                theme.templates()
            }
            _ => format!("/{}", project.display()),
        }
    }

    /// The card template's path on disk, for the fingerprint that ties every
    /// page's card to the template that drew it.
    fn card_template(&self) -> PathBuf {
        self.config.templates.join(&self.config.cards.template)
    }

    /// Drop orphaned outputs from earlier builds (a removed page or taxonomy
    /// term, a renamed permalink) so `dist` never serves stale files.
    ///
    /// Gated on `clean` so a user managing `dist` by hand can opt out. The
    /// keep-set is every file this build produced: page HTML, static
    /// passthrough, generated files. The asset tree is regenerated wholesale, so
    /// the prune skips it. Runs before `after` hooks, whose outputs (Pagefind..)
    /// are not ours to prune.
    fn sweep(&self, outputs: &[Output], statics: &Copied, generated: &Generated) -> Result<()> {
        if !self.config.clean {
            return Ok(());
        }
        // A card belongs to its page whether or not this build re-rendered it,
        // so the keep set is derived from the pages, never from what was written.
        let cards = outputs
            .iter()
            .map(|out| out.page)
            .filter(|page| page.wants_card(&self.config))
            .map(|page| self.config.cards.path(&self.config.dist, &page.permalink));
        let keep: Vec<PathBuf> = outputs
            .iter()
            .map(|out| out.page.output.clone())
            .chain(cards)
            .chain(statics.paths.iter().cloned())
            .chain(generated.paths.iter().cloned())
            .collect();
        let pruned = prune::Prune::new(
            &self.config.dist,
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
        let pages = plan(&self.config, &self.project)?;
        debug!(
            pages = pages.len(),
            site = self.config.label(),
            "planned check"
        );
        let renderer = Renderer::new(
            &pages,
            AssetMap::default(),
            SrcSets::default(),
            self.project.root(),
        );
        let trees = self.trees(&pages);
        let tracked = self.project.tracked();
        let roots: Vec<Root> = tracked
            .iter()
            .map(|(base, tree)| Root { base, tree })
            .collect();
        let analyzer = Analyzer::new(roots, &self.project);
        // Prepare + compile every page inline (no cache split, no output).
        let rendered = self.render_pages("checking", pages.iter().collect(), ui, |page| {
            (
                page,
                self.prepare(page, &trees).and_then(|(id, text, fp)| {
                    self.compile(page, id, text, fp, &renderer, &analyzer)
                }),
            )
        })?;
        self.validate(&rendered, &[], true, ui)?;
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

    /// Report each compile outcome (page status lines and any typst warnings
    /// the compiler raised), returning the rendered pages or, after every
    /// failure has been reported, an error carrying *all* failed pages'
    /// diagnostics (a single failure propagates unchanged).
    /// Compile a batch of pages in parallel and reduce to their rendered
    /// outputs: a progress bar, a rayon map producing one `(page, outcome)` per
    /// item, then [`Engine::collect`] (status lines + diagnostics) and
    /// then [`Engine::collect`] (status lines + diagnostics). The shared spine of
    /// `build` (over the stale subset, already prepared) and `check` (every page,
    /// prepared inline); `outcome` supplies the only difference: how one item
    /// renders. Validation is the caller's, since only `build` has cached pages
    /// to replay.
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
    /// content fingerprint: a hash of the exact text typst compiles. A real
    /// page's body reaches the compiler through `#include` (a tracked file
    /// read, so the dependency cache covers its edits); only generated
    /// listings, which have no file, inline their body, and only their
    /// wrapper text needs fingerprinting. Built once and shared by the cache
    /// check and the compile.
    fn prepare(&self, page: &Page, trees: &Trees) -> Result<Prepared> {
        let sections = trees.get(&page.lang);
        let rooted = self.project.virtualize(&page.source)?;
        let Some(template) = &page.template else {
            let text = page.body.clone();
            let fingerprint = Hash::of_bytes(text.as_bytes());
            return Ok((FileId::new(rooted), text, fingerprint));
        };
        let taxonomies = crate::codegen::Typst(&page.taxonomies()).to_string();
        // prev/next sibling links, exposed to the template as `page.nav`. Part of
        // the wrapper text, so a neighbour's addition, removal, or retitling
        // refingerprints this page and rebuilds it: the cache stays correct.
        let nav = crate::codegen::Typst(&Self::nav(&page.siblings)).to_string();
        let translations = crate::codegen::Typst(&Self::translations(page)).to_string();
        let strings = crate::codegen::Typst(&self.strings(&page.lang)).to_string();
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
        let context = compile::layout::Context {
            data: bind,
            taxonomies: &taxonomies,
            nav: &nav,
            sections,
            lang: &page.lang,
            translations: &translations,
            strings: &strings,
        };
        let text = Layout::new(
            &self.template_root(template),
            template,
            &vpath,
            context,
            body,
        )
        .to_string();
        // hash the exact text typst compiles; the parse into a `Source` is
        // deferred to `compile`, run only for stale pages.
        let fingerprint = Hash::of_bytes(text.as_bytes());
        Ok((id, text, fingerprint))
    }

    /// One language's [`Section`] tree as a value: exposed to that language's
    /// templates as `page.sections` (the single source a site nav is built from,
    /// so it can't drift from the pages) and reused by the `baudelaire:sections`
    /// JS module. Each node is `(id, pages: ((url, title), ..), children: (..))`,
    /// one per content directory; generated listings are excluded.
    fn sections(&self, pages: &[Page], lang: &str) -> crate::codegen::Value {
        crate::codegen::Value::array(
            Section::tree(pages, &self.config, lang)
                .iter()
                .map(Section::value),
        )
    }

    /// The section tree as wrapper text for every built language, so each page
    /// embeds its own language's nav. Built once and shared by every page.
    fn trees(&self, pages: &[Page]) -> Trees {
        Trees(
            self.config
                .langs()
                .iter()
                .map(|lang| {
                    let tree = crate::codegen::Typst(&self.sections(pages, lang)).to_string();
                    ((*lang).to_owned(), tree)
                })
                .collect(),
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

    /// A page's translations as an array value:
    /// `((lang: .., url: .., title: ..), ..)`, exposed to the template as
    /// `page.translations` for a language switcher. Empty on a single-language
    /// site.
    fn translations(page: &Page) -> crate::codegen::Value {
        use crate::codegen::Value;
        Value::array(page.translations.iter().map(|t| {
            Value::dict([
                ("lang", Value::str(&t.lang)),
                ("url", Value::str(&t.url)),
                ("title", Value::str(&t.title)),
            ])
        }))
    }

    /// A language's UI-string table as a dict value, exposed to the template as
    /// `page.strings`. Empty for a language with no `strings` block.
    fn strings(&self, lang: &str) -> crate::codegen::Value {
        use crate::codegen::Value;
        Value::dict(
            self.config
                .strings(lang)
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        )
    }

    /// A page's project-root-absolute virtual path (`/content/posts/a.typ`):
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
        analyzer: &Analyzer,
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
        let warnings = self.diagnostics(warnings, page, &source, world.inner());
        let mut doc = compiled.output.map_err(|errs| {
            BaudelaireErrorKind::TypstCompile(self.diagnostics(errs, page, &source, world.inner()))
        })?;
        let rewrite = renderer.rewrite(&mut doc, page, &self.config);
        let options = HtmlOptions {
            pretty: self.config.html.pretty,
        };
        // Shared by both serializations below, so a failure in either reports
        // with the page's own spans.
        let serialization_failed = |errs| {
            BaudelaireErrorKind::TypstHtml(self.diagnostics(errs, page, &source, world.inner()))
        };
        let html = typst_html::html(&doc, &options).map_err(&serialization_failed)?;
        // Only the single-file export consumes these, and capturing them costs
        // a second pass over the DOM, so nothing else pays for it.
        let fragments = self
            .config
            .standalone
            .enabled
            .then(|| Fragments::capture(&doc, &options).map_err(&serialization_failed))
            .transpose()?;
        // A card is a second, paged compile of the same page, so only a stale
        // page pays for one. A cache hit keeps the file already in `dist`.
        #[cfg(feature = "cards")]
        let card = page
            .wants_card(&self.config)
            .then(|| Card::render(&self.project, &self.config, page))
            .transpose()?;
        let deps = self.project.dependencies(&world);
        // Which injected values (`sys.inputs.baudelaire.*`) the page read, across
        // its own source and every `.typ` it depends on: the fine-grained
        // metadata dependency set.
        let mut reads = analyzer.reads(&source, &deps);
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
            outputs: Outputs {
                images: rewrite.images,
                broken: rewrite.broken,
                fragments,
            },
            external: rewrite.external,
            #[cfg(feature = "cards")]
            card,
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
        outbound: bool,
        ui: &Ui,
    ) -> Result<()> {
        // Cached pages contribute no outbound links: nothing recompiled them, so
        // nothing re-read their anchors. Only `check` asks for them, and it never
        // serves a page from cache.
        let fresh = rendered
            .iter()
            .map(|r| (r.page, &r.outputs, r.external.as_slice()));
        let reused = cached
            .iter()
            .map(|(page, _, outputs)| (*page, outputs, &[] as &[String]));
        let pages: Vec<CheckedPage> = fresh
            .chain(reused)
            .map(|(page, outputs, external)| CheckedPage {
                label: self.relative(page),
                source: &page.source,
                broken: &outputs.broken,
                external,
            })
            .collect();
        let site = Compiled {
            config: &self.config,
            pages: &pages,
        };
        Links::run(&site, ui)?;
        if outbound && self.config.links.external {
            External::run(&site, ui)?;
        }
        Ok(())
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
    reads: Reads,
    /// The render pass's own results (externalized images, broken links), the
    /// same shape the cache stores and replays for a hit.
    outputs: Outputs,
    /// Outbound `http(s)` links the page carries, for `check --external`. Not
    /// cached: only a fresh compile collects them, and only `check` reads them.
    external: Vec<String>,
    /// The page's rendered social card as PNG bytes, when it wanted one.
    #[cfg(feature = "cards")]
    card: Option<Vec<u8>>,
    warnings: Vec<TypstSourceDiagnostic>,
}
