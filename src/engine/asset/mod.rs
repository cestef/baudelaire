//! The asset pipeline: classify each file under `config.paths.assets`, transform it
//! through the [`Handler`] that claims it, and write the result into `dist`.
//!
//! This module is the orchestrator: bucket the files, run the handlers in phase
//! order, memoize what is memoizable, fingerprint and write. The handler
//! protocol and the registry of kinds live in [`handler`], so a new asset kind
//! never touches the pipeline.
//!
//! Two phases order the work. [`Phase::Early`] handlers (scripts, images, plain
//! copies) run first, so their fingerprint renames populate the [`AssetMap`].
//! [`Phase::Late`] handlers (stylesheets) run second, rewriting their `url()` /
//! `@import` references to the final hashed names now in the map.

#[cfg(feature = "css")]
mod css;
#[cfg(feature = "images")]
mod exif;
pub(in crate::engine) mod handler;
#[cfg(feature = "images")]
pub(in crate::engine) mod image;
#[cfg(feature = "js")]
mod js;
pub(in crate::engine) mod memo;
#[cfg(feature = "js")]
mod module;
mod owned;
#[cfg(feature = "sass")]
mod sass;
mod sourcemap;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::config::Config;
#[cfg(feature = "js")]
use crate::content::Page;
use crate::error::Result;
use crate::fs;
use crate::graph::AssetName;
use rayon::prelude::*;

use crate::engine::layers::{Layered, Layers};
use crate::render::{AssetMap, Emitted, SrcSets};
use crate::theme::Theme;
use memo::Memo;

// Re-exported for the handler modules (and [`memo`]), which name these as
// `super::*`; the protocol itself lives in [`handler`].
use crate::config::SourceMaps;
use handler::{Ctx, Handler, PathExt, Phase, Private, Produced, Variant, builtin};
use sourcemap::SourceMap;

#[cfg(feature = "js")]
use js::Js;
#[cfg(feature = "js")]
use module::ModuleCx;

/// The TypeScript declarations for the `baudelaire:*` modules, written by
/// `baudelaire packages` beside the typst ones.
#[cfg(feature = "js")]
pub use module::Declarations;

/// The outcome of processing the asset tree: the request->served URL map (only
/// entries renamed by fingerprinting appear), the count of files emitted
/// (partials excluded), and their total byte size.
#[derive(Default)]
pub struct Processed {
    pub map: AssetMap,
    /// Responsive width variants, keyed by source path: the render layer's
    /// `srcset` source.
    pub srcsets: SrcSets,
    /// Every file written and its size, for the site-wide weight check. The
    /// pipeline is the only thing that knows both, and it knows them here.
    pub emitted: Emitted,
    pub count: usize,
    pub bytes: u64,
    /// The [`Owned`] assets this build named but has not written; see
    /// [`Assets::generated`].
    pub deferred: Vec<Deferred>,
}

/// An asset the build provides itself, named and digested but not yet on disk.
///
/// Reserved during [`Assets::process`] so a page can link it and be stamped with
/// its digest, and written by [`Assets::requested`] only if a page did. An
/// unwritten one costs a map entry and nothing else, which is the point: a site
/// with no equation on any page must not ship a stylesheet for equations.
pub struct Deferred {
    /// Path relative to the asset root: how a page names it, and the key a
    /// render pass records when it asks for one.
    rel: PathBuf,
    /// The fingerprinted path it is served from, settled here because the bytes
    /// are settled here.
    dst: PathBuf,
    bytes: std::borrow::Cow<'static, [u8]>,
}

/// One file's rendered outputs, held until the serial emit pass writes them.
struct Render {
    /// Source path relative to the asset root.
    rel: PathBuf,
    /// Where it is served from, which a handler may rename (`.ts` -> `.js`).
    served: PathBuf,
    primary: Option<Vec<u8>>,
    /// The source map for `primary`, still unlinked to it: naming the map is
    /// the emit pass's, since the name it is linked under is only settled once
    /// `primary` has been fingerprinted.
    map: Option<Vec<u8>>,
    variants: Vec<Variant>,
}

/// The site data the JS bundler needs to serve its `baudelaire:*` virtual
/// modules, captured up front and combined with the finalized [`AssetMap`] at
/// bundle time. Bundled into one value so [`Assets`] carries a single js-gated
/// field rather than a feature-varying constructor arity.
#[cfg(feature = "js")]
pub struct JsCtx<'a> {
    /// The planned pages, exposed to `baudelaire:pages` / `:taxonomies` / `:feed`.
    pub pages: &'a [Page],
    /// The `sys.inputs.baudelaire` value, so `baudelaire:site` / `:config` serve
    /// the same build context sub-trees the templates get (not a rebuild).
    pub context: &'a crate::codegen::Value,
    /// The section tree value, so `baudelaire:sections` reuses what
    /// `page.sections` already built instead of recomputing it.
    pub sections: &'a crate::codegen::Value,
}

/// The asset pipeline over one site's asset directory.
pub struct Assets<'a> {
    config: &'a Config,
    /// The site data the JS bundler serves through its `baudelaire:*` virtual
    /// modules, present only under the `js` feature, since nothing else reads it.
    #[cfg(feature = "js")]
    js: JsCtx<'a>,
    /// Where assets are read from: the theme's tree beneath the project's, so a
    /// theme ships a stylesheet the site can replace file by file.
    sources: Layers,
    /// Where this build writes, published over `dist/assets` by
    /// [`Assets::publish`] once the build is far enough along to be consistent.
    dst: PathBuf,
    /// Cross-build memo of processed bytes, so an unchanged image is not
    /// re-encoded on every build.
    memo: Memo,
    /// URL prefix the assets are served under, e.g. `/assets`.
    prefix: String,
}

impl<'a> Assets<'a> {
    pub fn new(
        config: &'a Config,
        theme: Option<&Theme>,
        #[cfg(feature = "js")] js: JsCtx<'a>,
    ) -> Self {
        Self {
            config,
            #[cfg(feature = "js")]
            js,
            sources: Layers::new(theme.map(Theme::assets), &config.paths.assets),
            dst: config.asset_staging(),
            memo: Memo::new(config),
            prefix: config.asset_prefix(),
        }
    }

    /// Move the staged tree into its served place, replacing whatever the
    /// previous build left there. Called once every page is on disk naming the
    /// new asset filenames; see [`Config::asset_staging`].
    ///
    /// A rename, so there is no window in which half the assets exist. The
    /// served tree is dropped even when this build staged nothing: the pipeline
    /// owns it end to end and the prune pass deliberately skips it, so anything
    /// left behind would never be collected.
    pub fn publish(&self) -> Result<()> {
        let served = self.config.asset_dist();
        if served.exists() {
            fs::remove_dir_all(&served)?;
        }
        if self.dst.exists() {
            fs::rename(&self.dst, &served)?;
        }
        Ok(())
    }

    /// Process every asset into `dist`, returning the [`Processed`] summary.
    /// The staging tree is *not* cleared here: [`Engine::build`] clears it at
    /// the start of the build, before the static copy seeds it with whatever
    /// `static/` places inside the asset directory.
    ///
    /// [`Engine::build`]: crate::engine::Engine::build
    pub fn process(&self) -> Result<Processed> {
        let mut out = Processed {
            map: AssetMap::new(self.prefix.clone()),
            emitted: Emitted::new(self.config.base_path().to_owned()),
            ..Processed::default()
        };
        // What the tree holds for the build's own use never reaches `dist`.
        let sources: Vec<Layered> = self
            .sources
            .files()?
            .into_iter()
            .filter(|file| !Private::covers(&file.rel, self.config))
            .collect();
        self.generated(&sources, &mut out);
        if sources.is_empty() {
            return Ok(out);
        }
        let handlers = builtin();
        // Bucket every file under the first handler that claims it.
        let mut buckets: Vec<Vec<Layered>> = handlers.iter().map(|_| Vec::new()).collect();
        for file in sources {
            let idx = handlers
                .iter()
                .position(|h| h.claims(&file.path, self.config))
                .expect("Verbatim claims every file");
            buckets[idx].push(file);
        }
        // Early then Late: non-bundle phases run without a bundler, so their
        // fingerprint renames land in the map before anything reads it.
        let ctx = self.ctx();
        for phase in [Phase::Early, Phase::Late] {
            self.phase(phase, &handlers, &mut buckets, &ctx, &mut out)?;
        }
        // Bundle phase last: build the bundler now that the map is final, so a
        // `baudelaire:assets` import resolves every asset processed above.
        #[cfg(feature = "js")]
        {
            let bundling = handlers
                .iter()
                .zip(&buckets)
                .any(|(h, b)| h.phase() == Phase::Bundle && !b.is_empty());
            if bundling {
                let js = {
                    let cx = ModuleCx {
                        config: self.config,
                        pages: self.js.pages,
                        assets: &out.map,
                        context: self.js.context,
                        sections: self.js.sections,
                    };
                    Js::new(&cx)?
                };
                // The same context the other phases ran against, with the
                // bundler attached: one place spells the field list.
                let ctx = Ctx {
                    bundler: Some(&js),
                    ..self.ctx()
                };
                self.phase(Phase::Bundle, &handlers, &mut buckets, &ctx, &mut out)?;
            }
        }
        Ok(out)
    }

    /// Name every [`Owned`] asset this build serves, beneath the layers a site
    /// and its theme provide.
    ///
    /// The override rule is the layer stack's, stated here once rather than in
    /// each impl: a tree holding its own file at that path keeps it, exactly as
    /// it would override one a theme shipped. From there the two are
    /// indistinguishable, which is the whole reason the name is settled here: a
    /// page links the authored spelling either way, and the fingerprint pass
    /// rewrites it through the same map entry.
    ///
    /// Named, digested, and *not* written. Whether a page wants one is not
    /// knowable before the pages have rendered, and an owned asset is not the
    /// site's own file: nobody put it in the tree, so nobody would wonder why
    /// `dist` holds a stylesheet no page asks for. [`Assets::requested`] writes
    /// the ones that were asked for, once that is known.
    fn generated(&self, sources: &[Layered], out: &mut Processed) {
        let ctx = self.ctx();
        for asset in owned::builtin() {
            let rel = Path::new(asset.rel());
            if !asset.enabled(self.config) || sources.iter().any(|file| file.rel == rel) {
                continue;
            }
            let bytes = asset.bytes(self.config);
            let dst = self.fingerprint(rel, &bytes);
            // Unconditionally mapped, even when fingerprinting leaves the name
            // alone: the entry is what makes the reference *resolve*, and a page
            // records which owned assets it asked for by the same key.
            out.map.insert(ctx.url(rel), ctx.url(&dst));
            out.emitted.insert(ctx.url(&dst), &bytes, self.config.sri());
            out.deferred.push(Deferred {
                rel: rel.to_path_buf(),
                dst,
                bytes,
            });
        }
    }

    /// Write the [`Deferred`] assets the rendered pages asked for, keyed by the
    /// path relative to the asset root that both sides name them by.
    ///
    /// Runs where the externalized images are written, and for the same reason:
    /// the asset tree is regenerated every build, so a page served from cache
    /// has to keep its file alive just as a freshly compiled one does.
    pub fn requested(&self, deferred: &[Deferred], wanted: &BTreeSet<String>) -> Result<Processed> {
        let mut out = Processed {
            map: AssetMap::new(self.prefix.clone()),
            emitted: Emitted::new(self.config.base_path().to_owned()),
            ..Processed::default()
        };
        let ctx = self.ctx();
        for asset in deferred {
            if wanted.contains(&asset.rel.to_string_lossy().replace('\\', "/")) {
                self.write(&ctx, &asset.rel, &asset.dst, &asset.bytes, &mut out)?;
            }
        }
        Ok(out)
    }

    /// Run every handler belonging to `phase` over the files bucketed for it,
    /// draining each bucket as it goes so a later phase sees only its own.
    fn phase(
        &self,
        phase: Phase,
        handlers: &[Box<dyn Handler>],
        buckets: &mut [Vec<Layered>],
        ctx: &Ctx,
        out: &mut Processed,
    ) -> Result<()> {
        for (handler, bucket) in handlers.iter().zip(buckets) {
            if handler.phase() == phase && !bucket.is_empty() {
                self.run(handler.as_ref(), std::mem::take(bucket), ctx, out)?;
            }
        }
        Ok(())
    }

    /// A render context borrowing this pipeline's config/paths. The bundle phase
    /// (js feature) builds its own [`Ctx`] with the bundler attached.
    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            config: self.config,
            #[cfg(feature = "sass")]
            roots: self.sources.search(),
            #[cfg(feature = "js")]
            bundler: None,
        }
    }

    /// Render one handler's files (in its chosen order) and emit each result.
    fn run(
        &self,
        handler: &dyn Handler,
        files: Vec<Layered>,
        ctx: &Ctx,
        out: &mut Processed,
    ) -> Result<()> {
        let files = handler.order(files, ctx);
        match handler.pure() {
            // A pure handler's output is decided by the file's own bytes, so its
            // files are independent: the expensive half (re-encoding an image)
            // runs across the pool, and the writes and map inserts follow in
            // order, single-threaded.
            true => {
                let rendered: Vec<Render> = files
                    .par_iter()
                    .map(|file| self.render(handler, file, ctx, &out.map))
                    .collect::<Result<_>>()?;
                for render in rendered {
                    self.finish(handler, render, ctx, out)?;
                }
            }
            // An impure one reads the map, which is what `Handler::render`
            // promises holds every asset processed so far. Each file's insert has
            // to land before the next one renders, or a handler whose files
            // reference each other sees none of them: a stylesheet's `@import`
            // resolved to nothing however carefully `order` had sorted it, and
            // fell back to the unhashed name it was written with.
            false => {
                for file in &files {
                    let render = self.render(handler, file, ctx, &out.map)?;
                    self.finish(handler, render, ctx, out)?;
                }
            }
        }
        Ok(())
    }

    /// Write one rendered file and record what it was served as.
    fn finish(
        &self,
        handler: &dyn Handler,
        render: Render,
        ctx: &Ctx,
        out: &mut Processed,
    ) -> Result<()> {
        let Render {
            rel,
            served,
            primary,
            map,
            variants,
        } = render;
        if let Some(bytes) = primary {
            let posture = handler.sourcemaps(self.config);
            let dst = self.mapped(ctx, &served, bytes, map, posture, out)?;
            // A renamed asset is referenced by *either* name: authors write
            // `main.js` for a bundle, but `main.ts` is what is on disk and
            // what an editor completes. Map both, or one of the two spellings
            // silently keeps pointing at a file that was never written.
            if served != rel {
                out.map.insert(ctx.url(&rel), ctx.url(&dst));
            }
        }
        // Responsive variants: write each downscaled copy (the source's own
        // width carries no bytes, having been emitted above) and record it
        // as a `srcset` candidate against the source's URL.
        for variant in variants {
            if let Some(bytes) = &variant.bytes {
                self.emit(ctx, &variant.rel, bytes, out)?;
            }
            out.srcsets
                .record(ctx.url(&rel), variant.width, ctx.url(&variant.rel));
        }
        Ok(())
    }

    /// Produce one file's outputs, from the memo when the handler is pure and
    /// nothing that shapes them has changed.
    fn render(
        &self,
        handler: &dyn Handler,
        source: &Layered,
        ctx: &Ctx,
        map: &AssetMap,
    ) -> Result<Render> {
        let Layered { rel, path: file } = source;
        let rel = rel.clone();
        // Render against the source path (stylesheets resolve their relative
        // references from it), emit under the served one.
        let served = handler.rename(&rel);
        let key = match handler.pure() {
            true => Some(self.memo.key(&fs::read(file)?, &rel)),
            false => None,
        };
        if let Some((primary, variants)) = key.as_ref().and_then(|key| self.memo.get(key)) {
            return Ok(Render {
                rel,
                served,
                primary,
                // A memoized handler is a pure one, and a pure handler is one
                // whose output is its own bytes: nothing that builds a source
                // map qualifies, so there is none to have stored.
                map: None,
                variants,
            });
        }
        let Produced { bytes, map } = handler.render(file, &rel, map, ctx)?;
        let variants = handler.variants(file, &rel, ctx)?;
        if let Some(key) = &key {
            self.memo.put(key, bytes.as_deref(), &variants);
        }
        Ok(Render {
            rel,
            served,
            primary: bytes,
            map,
            variants,
        })
    }

    /// Write an asset together with the source map it was built against, each
    /// naming the other. Returns the path the asset itself was written to.
    ///
    /// The order is the whole of it. The name is fingerprinted from the bytes
    /// the handler produced, *before* the `sourceMappingURL` comment is
    /// appended, so the comment cannot change the name it has just been made to
    /// point at. The map is then named after that settled name, and the linked
    /// bytes are what gets hashed for `integrity` and weighed for a budget,
    /// because they are what a browser fetches.
    ///
    /// The map itself is deliberately not fingerprinted: it is already named
    /// after a file that is, so it turns over whenever that one does, and a map
    /// is fetched by name from the comment rather than from any page.
    fn mapped(
        &self,
        ctx: &Ctx,
        rel: &Path,
        bytes: Vec<u8>,
        map: Option<Vec<u8>>,
        posture: SourceMaps,
        out: &mut Processed,
    ) -> Result<PathBuf> {
        let dst = self.fingerprint(rel, &bytes);
        let Some(map) = map else {
            self.write(ctx, rel, &dst, &bytes, out)?;
            return Ok(dst);
        };
        // An inline map is written into the asset instead of beside it, so
        // there is no second file and nothing else to name.
        if posture.inline() {
            let bytes = SourceMap::inlined(bytes, &dst, &map);
            self.write(ctx, rel, &dst, &bytes, out)?;
            return Ok(dst);
        }
        let bytes = match posture.linked() {
            true => SourceMap::linked(bytes, &dst),
            // Hidden: the map is written and the asset says nothing about it.
            false => bytes,
        };
        self.write(ctx, rel, &dst, &bytes, out)?;
        let at = SourceMap::beside(&dst);
        self.write(ctx, &at, &at, &map, out)?;
        Ok(dst)
    }

    /// Fingerprint (when enabled) and write `bytes` for the asset at `rel`,
    /// recording the request->served URL mapping when the name changed. Returns
    /// the path actually written.
    fn emit(&self, ctx: &Ctx, rel: &Path, bytes: &[u8], out: &mut Processed) -> Result<PathBuf> {
        let dst = self.fingerprint(rel, bytes);
        self.write(ctx, rel, &dst, bytes, out)?;
        Ok(dst)
    }

    /// Write `bytes` to the already-settled `dst`, recording the request->served
    /// URL mapping when the name changed.
    ///
    /// Split from [`Assets::emit`] because a mapped asset has to choose its name
    /// before it has its final bytes, which is the one case that cannot go
    /// through fingerprint-then-write in a single step.
    fn write(
        &self,
        ctx: &Ctx,
        rel: &Path,
        dst: &Path,
        bytes: &[u8],
        out: &mut Processed,
    ) -> Result<()> {
        fs::write_all(self.dst.join(dst), bytes)?;
        out.count += 1;
        out.bytes += bytes.len() as u64;
        out.emitted.insert(ctx.url(dst), bytes, self.config.sri());
        if dst != rel {
            out.map.insert(ctx.url(rel), ctx.url(dst));
        }
        Ok(())
    }

    /// The relative output path for an asset, splicing a content hash into the
    /// filename when fingerprinting is enabled (`app.css` -> `app.<hash>.css`).
    /// The digest and the splice are [`AssetName`]'s, shared with the render
    /// pass so an externalized image is named like any other asset.
    fn fingerprint(&self, rel: &Path, bytes: &[u8]) -> PathBuf {
        match self.config.assets.fingerprint {
            true => rel.suffixed(&AssetName::digest(bytes)),
            false => rel.to_path_buf(),
        }
    }
}
