//! The asset pipeline: classify each file under `config.assets`, transform it
//! through the [`Handler`] that claims it, and write the result into `dist`.
//!
//! Each asset kind is one handler ([`Stylesheet`], [`Script`], [`Raster`], or
//! the fallback [`Verbatim`] copy), registered in [`builtin`]. A handler owns
//! its kind end to end: which files it claims and how their bytes are produced.
//! Adding a kind is a new `Handler` impl and one line in `builtin`; nothing in
//! the orchestrator changes.
//!
//! Two phases order the work. [`Phase::Early`] handlers (scripts, images, plain
//! copies) run first, so their fingerprint renames populate the [`AssetMap`].
//! [`Phase::Late`] handlers (stylesheets) run second, rewriting their `url()` /
//! `@import` references to the final hashed names now in the map.

#[cfg(feature = "css")]
mod css;
#[cfg(feature = "images")]
mod exif;
#[cfg(feature = "images")]
mod image;
#[cfg(feature = "js")]
mod js;
mod memo;
#[cfg(feature = "js")]
mod module;

#[cfg(feature = "css")]
use std::path::Component;
use std::path::{Path, PathBuf};

use crate::config::Config;
#[cfg(feature = "js")]
use crate::content::Page;
use crate::error::Result;
use crate::fs;
use crate::graph::Hash;
use rayon::prelude::*;

use crate::engine::layers::{Layered, Layers};
use crate::render::{AssetMap, SrcSets};
use crate::theme::Theme;
use memo::Memo;

#[cfg(feature = "css")]
use css::Stylesheet;
#[cfg(feature = "images")]
use image::Raster;
#[cfg(feature = "js")]
use js::{Js, Script};
#[cfg(feature = "js")]
use module::ModuleCx;

/// Length of the hex fingerprint spliced into asset filenames. 16 hex chars =
/// 64 bits of blake3: collision-free in practice for a site's asset set.
///
/// Shared with [`crate::render`], which names externalized images the same way:
/// an asset that arrives through the render pass must be indistinguishable from
/// one the pipeline emitted.
pub(crate) const FINGERPRINT_LEN: usize = 16;

/// The outcome of processing the asset tree: the request->served URL map (only
/// entries renamed by fingerprinting appear), the count of files emitted
/// (partials excluded), and their total byte size.
#[derive(Default)]
pub struct Processed {
    pub map: AssetMap,
    /// Responsive width variants, keyed by source path: the render layer's
    /// `srcset` source.
    pub srcsets: SrcSets,
    pub count: usize,
    pub bytes: u64,
}

/// When a handler runs, in order. `Early` assets (images, copies) provide the
/// fingerprinted names others reference; `Late` assets (stylesheets) rewrite
/// their references against them; `Bundle` assets (scripts) run last, so a
/// bundle importing `baudelaire:assets` sees the finalized map.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Early,
    Late,
    #[cfg(feature = "js")]
    Bundle,
}

/// The read-only context a handler renders against: the config, the served URL
/// prefix, and the shared JS bundler. The accumulating [`AssetMap`] is passed to
/// [`Handler::render`] separately, so the pipeline can keep mutating it between
/// calls.
struct Ctx<'a> {
    /// The site config: read by the css and image handlers for their options.
    #[cfg(any(feature = "css", feature = "images"))]
    config: &'a Config,
    prefix: &'a str,
    #[cfg(feature = "js")]
    bundler: Option<&'a Js>,
}

impl Ctx<'_> {
    /// The served URL for a relative asset path, e.g. `/assets/css/app.css`.
    fn url(&self, rel: &Path) -> String {
        let rel = rel.to_string_lossy().replace('\\', "/");
        format!("{}/{rel}", self.prefix)
    }

    /// Lexically normalize a virtual asset path, collapsing `.`/`..` segments
    /// (the assets live under `dist`, so there is nothing to canonicalize).
    /// `None` when the path walks out of the asset root.
    ///
    /// Fallible because `PathBuf::pop` on an empty buffer is a silent no-op:
    /// `url(../x.png)` in `assets/a.css` normalized to `assets/x.png` and so
    /// resolved to a *different, real* file whenever one happened to exist.
    #[cfg(feature = "css")]
    fn normalize(path: &Path) -> Option<PathBuf> {
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                Component::ParentDir if !out.pop() => return None,
                Component::ParentDir | Component::CurDir => {}
                other => out.push(other),
            }
        }
        Some(out)
    }
}

/// The lowercase-comparable extension of a path, or `""` when it has none.
/// A private extension trait so the handlers share one spelling of it. Only the
/// css/js/image handlers claim by extension, so it is absent from a copy-only
/// (all-features-off) build.
#[cfg(any(feature = "css", feature = "js", feature = "images"))]
pub(super) trait PathExt {
    fn ext(&self) -> &str;
}

#[cfg(any(feature = "css", feature = "js", feature = "images"))]
impl PathExt for Path {
    fn ext(&self) -> &str {
        self.extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
    }
}

/// One asset-processing strategy: which files it claims, when it runs, and how a
/// claimed file becomes its emitted bytes.
trait Handler: Sync {
    /// Whether this handler processes `file`. The first handler in [`builtin`]
    /// to claim a file owns it, so specific handlers come first and [`Verbatim`]
    /// claims whatever is left.
    fn claims(&self, file: &Path, config: &Config) -> bool;

    /// When this handler runs relative to the others.
    fn phase(&self) -> Phase {
        Phase::Early
    }

    /// Whether this handler's output is a pure function of the file's own bytes
    /// and the config, and so can be memoized across builds.
    ///
    /// False by default, and deliberately so: a stylesheet rewrites references
    /// to *other* assets' hashed names and a script bundles a whole import
    /// graph, so neither is determined by the bytes in front of it.
    fn pure(&self) -> bool {
        false
    }

    /// Reorder this handler's files before rendering. The default keeps input
    /// order; stylesheets override it to fingerprint an imported sheet before
    /// its importer.
    fn order(&self, files: Vec<Layered>, _ctx: &Ctx) -> Vec<Layered> {
        files
    }

    /// The served path for a claimed file, when this handler's output is no
    /// longer the same kind of file as its source. Default: unchanged.
    ///
    /// Scripts use it: a bundled `.ts` entry holds JavaScript, and writing it as
    /// `app.<hash>.ts` left the served file under a MIME type browsers refuse
    /// for `type=module`, keyed in the asset map under a name no author writes.
    fn rename(&self, rel: &Path) -> PathBuf {
        rel.to_path_buf()
    }

    /// Transform `file` (relative path `rel`) into the bytes written to `dist`,
    /// or `None` to emit nothing: a script partial pulled in only through
    /// imports. `map` holds the served names of every asset processed so far.
    fn render(&self, file: &Path, rel: &Path, map: &AssetMap, ctx: &Ctx)
    -> Result<Option<Vec<u8>>>;

    /// Responsive width variants derived from `file`, beyond the primary
    /// [`render`](Handler::render) output: the raster handler's downscaled
    /// copies. The pipeline writes each variant and records the `srcset`
    /// manifest from their widths. Default: none.
    ///
    /// [`Handler::render`]: Handler::render
    fn variants(&self, _file: &Path, _rel: &Path, _ctx: &Ctx) -> Result<Vec<Variant>> {
        Ok(Vec::new())
    }
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

/// One responsive candidate a handler derives from a source image: a target
/// `width`, its output path `rel`, and the `bytes` to write. `bytes` is `None`
/// for the source's own width, whose bytes are the handler's primary output; it
/// still becomes the largest `srcset` candidate.
pub(super) struct Variant {
    pub rel: PathBuf,
    pub width: u32,
    pub bytes: Option<Vec<u8>>,
}

/// The registered handlers, in claim priority: [`Verbatim`] is last because it
/// claims every file. [`Script`] is present only under the `js` feature; without
/// it, `.js` files fall through to [`Verbatim`] and are copied unbundled.
fn builtin() -> Vec<Box<dyn Handler>> {
    vec![
        #[cfg(feature = "css")]
        Box::new(Stylesheet),
        #[cfg(feature = "js")]
        Box::new(Script),
        #[cfg(feature = "images")]
        Box::new(Raster),
        Box::new(Verbatim),
    ]
}

/// The fallback handler: copies a file byte-for-byte. Claims everything, so it
/// comes last in [`builtin`].
struct Verbatim;

impl Handler for Verbatim {
    fn claims(&self, _file: &Path, _config: &Config) -> bool {
        true
    }

    fn render(
        &self,
        file: &Path,
        _rel: &Path,
        _map: &AssetMap,
        _ctx: &Ctx,
    ) -> Result<Option<Vec<u8>>> {
        Ok(Some(fs::read(file)?))
    }
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
            sources: Layers::new(theme.map(Theme::assets), &config.assets),
            dst: config.asset_staging(),
            memo: Memo::new(config),
            prefix: format!("/{}", config.asset_name()),
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
        let mut out = Processed::default();
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
            for (handler, bucket) in handlers.iter().zip(&mut buckets) {
                if handler.phase() == phase && !bucket.is_empty() {
                    self.run(handler.as_ref(), std::mem::take(bucket), &ctx, &mut out)?;
                }
            }
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
                let ctx = Ctx {
                    #[cfg(any(feature = "css", feature = "images"))]
                    config: self.config,
                    prefix: &self.prefix,
                    bundler: Some(&js),
                };
                for (handler, bucket) in handlers.iter().zip(&mut buckets) {
                    if handler.phase() == Phase::Bundle && !bucket.is_empty() {
                        self.run(handler.as_ref(), std::mem::take(bucket), &ctx, &mut out)?;
                    }
                }
            }
        }
        Ok(out)
    }

    /// A render context borrowing this pipeline's config/paths. The bundle phase
    /// (js feature) builds its own [`Ctx`] with the bundler attached.
    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            #[cfg(any(feature = "css", feature = "images"))]
            config: self.config,
            prefix: &self.prefix,
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
        if dst != rel {
            out.map.insert(ctx.url(rel), ctx.url(&dst));
        }
        Ok(dst)
    }

    /// The relative output path for an asset, splicing a content hash into the
    /// filename when fingerprinting is enabled (`app.css` -> `app.<hash>.css`).
    fn fingerprint(&self, rel: &Path, bytes: &[u8]) -> PathBuf {
        if !self.config.asset.fingerprint {
            return rel.to_path_buf();
        }
        let hash = Hash::of_bytes(bytes);
        let digest = hash.short(FINGERPRINT_LEN);
        let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let name = match rel.extension().and_then(|e| e.to_str()) {
            Some(ext) => format!("{stem}.{digest}.{ext}"),
            None => format!("{stem}.{digest}"),
        };
        rel.with_file_name(name)
    }
}

#[cfg(all(test, feature = "css"))]
mod tests {
    use super::Ctx;
    use std::path::{Path, PathBuf};

    /// A `..` that walks out of the asset root must not be absorbed: it used to
    /// normalize to a sibling inside the root and resolve to a different, real
    /// file.
    #[test]
    fn normalize_rejects_a_path_escaping_the_asset_root() {
        assert_eq!(Ctx::normalize(Path::new("../x.png")), None);
        assert_eq!(Ctx::normalize(Path::new("css/../../x.png")), None);
    }

    #[test]
    fn normalize_collapses_interior_segments() {
        assert_eq!(
            Ctx::normalize(Path::new("css/../img/./logo.png")),
            Some(PathBuf::from("img/logo.png"))
        );
    }
}
