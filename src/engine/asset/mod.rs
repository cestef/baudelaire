//! The asset pipeline: classify each file under `config.paths.assets`, transform
//! it through the [`Handler`] that claims it, and write the result into `dist`.

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
use crate::owned;
use crate::render::{AssetMap, Emitted, SrcSets};
use crate::theme::Theme;
use memo::Memo;

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
    /// Every file written and its size, for the site-wide weight check.
    pub emitted: Emitted,
    pub count: usize,
    pub bytes: u64,
    /// The [`Owned`] assets this build named but has not written; see
    /// [`Assets::generated`].
    pub deferred: Vec<Deferred>,
}

/// An asset the build provides itself, named and digested but not yet on disk:
/// reserved during [`Assets::process`] so a page can link it, and written by
/// [`Assets::requested`] only if a page did.
pub struct Deferred {
    /// Path relative to the asset root: how a page names it, and the key a
    /// render pass records when it asks for one.
    rel: PathBuf,
    /// The fingerprinted path it is served from.
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
    /// The source map for `primary`, still unlinked to it: only the emit pass
    /// knows the name, once `primary` has been fingerprinted.
    map: Option<Vec<u8>>,
    variants: Vec<Variant>,
}

/// The site data the JS bundler needs to serve its `baudelaire:*` virtual
/// modules, captured up front and combined with the finalized [`AssetMap`] at
/// bundle time.
#[cfg(feature = "js")]
pub struct JsCtx<'a> {
    /// The planned pages, exposed to `baudelaire:pages` / `:taxonomies` / `:feed`.
    pub pages: &'a [Page],
    /// The `sys.inputs.baudelaire` value, so `baudelaire:site` / `:config` serve
    /// the same build context sub-trees the templates get.
    pub context: &'a crate::codegen::Value,
    /// The section tree value, as `page.sections` already built it.
    pub sections: &'a crate::codegen::Value,
}

/// The asset pipeline over one site's asset directory.
pub struct Assets<'a> {
    config: &'a Config,
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
    /// previous build left there. Call once every page is on disk naming the
    /// new asset filenames; the served tree is dropped even when this build
    /// staged nothing, since the prune pass skips it.
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
    /// the start of the build, before the static copy seeds it.
    ///
    /// [`Engine::build`]: crate::engine::Engine::build
    pub fn process(&self) -> Result<Processed> {
        let mut out = Processed {
            map: AssetMap::new(self.prefix.clone()),
            emitted: Emitted::new(self.config.base_path().to_owned()),
            ..Processed::default()
        };
        let sources: Vec<Layered> = self
            .sources
            .files()?
            .into_iter()
            .filter(|file| !Private::covers(&file.rel, self.config))
            .collect();
        self.generated(&sources, &mut out)?;
        if sources.is_empty() {
            return Ok(out);
        }
        let handlers = builtin();
        let mut buckets: Vec<Vec<Layered>> = handlers.iter().map(|_| Vec::new()).collect();
        for file in sources {
            let idx = handlers
                .iter()
                .position(|h| h.claims(&file.path, self.config))
                .expect("Verbatim claims every file");
            buckets[idx].push(file);
        }
        let ctx = self.ctx();
        for phase in [Phase::Early, Phase::Late] {
            self.phase(phase, &handlers, &mut buckets, &ctx, &mut out)?;
        }
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
                let ctx = Ctx {
                    bundler: Some(&js),
                    ..self.ctx()
                };
                self.phase(Phase::Bundle, &handlers, &mut buckets, &ctx, &mut out)?;
            }
        }
        Ok(out)
    }

    /// Name and digest every [`Owned`] asset this build serves, beneath the
    /// layers a site and its theme provide: a tree holding its own file at that
    /// path keeps it. Most are reserved rather than written, since whether a
    /// page wants one is not known until the pages have rendered.
    fn generated(&self, sources: &[Layered], out: &mut Processed) -> Result<()> {
        let ctx = self.ctx();
        for asset in owned::builtin() {
            let rel = asset.rel(self.config);
            if !asset.serves(self.config) || sources.iter().any(|file| file.rel == rel) {
                continue;
            }
            let bytes = asset.bytes(self.config)?;
            let dst = self.fingerprint(rel, &bytes);
            out.map.insert(ctx.url(rel), ctx.url(&dst));
            out.emitted.insert(ctx.url(&dst), &bytes, self.config.sri());
            if self.config.html.embed {
                self.write(&ctx, rel, &dst, &bytes, out)?;
                continue;
            }
            out.deferred.push(Deferred {
                rel: rel.to_path_buf(),
                dst,
                bytes,
            });
        }
        Ok(())
    }

    /// Write the [`Deferred`] assets the rendered pages asked for, keyed by the
    /// path relative to the asset root that both sides name them by. A page
    /// served from cache asks for its assets just as a freshly compiled one
    /// does, since the asset tree is regenerated every build.
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

    /// A render context borrowing this pipeline's config and paths; the bundle
    /// phase builds its own with the bundler attached.
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
        if handler.pure() {
            let rendered: Vec<Render> = files
                .par_iter()
                .map(|file| self.render(handler, file, ctx, &out.map))
                .collect::<Result<_>>()?;
            for render in rendered {
                self.finish(handler, render, ctx, out)?;
            }
        } else {
            for file in &files {
                let render = self.render(handler, file, ctx, &out.map)?;
                self.finish(handler, render, ctx, out)?;
            }
        }
        Ok(())
    }

    /// Write one rendered file and record what it was served as, under both its
    /// source and its served name when a handler renamed it: authors reference
    /// either spelling.
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
            if served != rel {
                out.map.insert(ctx.url(&rel), ctx.url(&dst));
            }
        }
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
        let served = handler.rename(&rel);
        let key = if handler.pure() {
            Some(self.memo.key(&fs::read(file)?, &rel))
        } else {
            None
        };
        if let Some((primary, variants)) = key.as_ref().and_then(|key| self.memo.get(key)) {
            return Ok(Render {
                rel,
                served,
                primary,
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
    /// naming the other, returning the path the asset was written to. The name
    /// is fingerprinted from the handler's bytes *before* the
    /// `sourceMappingURL` comment is appended, so the comment cannot change the
    /// name it has just been made to point at.
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
        if posture.inline() {
            let bytes = SourceMap::inlined(bytes, &dst, &map);
            self.write(ctx, rel, &dst, &bytes, out)?;
            return Ok(dst);
        }
        let bytes = if posture.linked() {
            SourceMap::linked(bytes, &dst)
        } else {
            bytes
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
    /// URL mapping when the name changed. Separate from [`Assets::emit`] for
    /// the mapped asset, which has to choose its name before its final bytes.
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
    fn fingerprint(&self, rel: &Path, bytes: &[u8]) -> PathBuf {
        if self.config.assets.fingerprint {
            rel.suffixed(&AssetName::digest(bytes))
        } else {
            rel.to_path_buf()
        }
    }
}
