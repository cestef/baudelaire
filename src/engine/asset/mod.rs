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
mod handler;
#[cfg(feature = "images")]
pub(in crate::engine) mod image;
#[cfg(feature = "js")]
mod js;
mod memo;
#[cfg(feature = "js")]
mod module;

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
use handler::{Ctx, Handler, PathExt, Phase, Variant, builtin};

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
}

/// One file's rendered outputs, held until the serial emit pass writes them.
struct Render {
    /// Source path relative to the asset root.
    rel: PathBuf,
    /// Where it is served from, which a handler may rename (`.ts` -> `.js`).
    served: PathBuf,
    primary: Option<Vec<u8>>,
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
        let sources = self.sources.files()?;
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
        // Render first, emit second. A pure handler's files are independent, so
        // the expensive half (re-encoding an image) runs across the pool while
        // the writes and the map inserts stay ordered and single-threaded.
        let rendered: Vec<Render> = match handler.pure() {
            true => files
                .par_iter()
                .map(|file| self.render(handler, file, ctx, &out.map))
                .collect::<Result<_>>()?,
            false => files
                .iter()
                .map(|file| self.render(handler, file, ctx, &out.map))
                .collect::<Result<_>>()?,
        };
        for render in rendered {
            let Render {
                rel,
                served,
                primary,
                variants,
            } = render;
            if let Some(bytes) = primary {
                let dst = self.emit(ctx, &served, &bytes, out)?;
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
                variants,
            });
        }
        let primary = handler.render(file, &rel, map, ctx)?;
        let variants = handler.variants(file, &rel, ctx)?;
        if let Some(key) = &key {
            self.memo.put(key, primary.as_deref(), &variants);
        }
        Ok(Render {
            rel,
            served,
            primary,
            variants,
        })
    }

    /// Fingerprint (when enabled) and write `bytes` for the asset at `rel`,
    /// recording the request->served URL mapping when the name changed. Returns
    /// the path actually written.
    fn emit(&self, ctx: &Ctx, rel: &Path, bytes: &[u8], out: &mut Processed) -> Result<PathBuf> {
        let dst = self.fingerprint(rel, bytes);
        fs::write_all(self.dst.join(&dst), bytes)?;
        out.count += 1;
        out.bytes += bytes.len() as u64;
        out.emitted.insert(ctx.url(&dst), bytes, self.config.sri());
        if dst != rel {
            out.map.insert(ctx.url(rel), ctx.url(&dst));
        }
        Ok(dst)
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
