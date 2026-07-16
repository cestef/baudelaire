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
mod image;
#[cfg(feature = "js")]
mod js;
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
use crate::render::AssetMap;

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
const FINGERPRINT_LEN: usize = 16;

/// The outcome of processing the asset tree: the request->served URL map (only
/// entries renamed by fingerprinting appear), the count of files emitted
/// (partials excluded), and their total byte size.
#[derive(Default)]
pub struct Processed {
    pub map: AssetMap,
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

/// The read-only context a handler renders against: the config, the source
/// root, the served URL prefix, and the shared JS bundler. The accumulating
/// [`AssetMap`] is passed to [`Handler::render`] separately, so the pipeline can
/// keep mutating it between calls.
struct Ctx<'a> {
    /// The site config: read by the css and image handlers for their options.
    #[cfg(any(feature = "css", feature = "images"))]
    config: &'a Config,
    prefix: &'a str,
    /// The stylesheet handler's extra context. Grouped into one value so [`Ctx`]
    /// carries a single css-gated field, mirroring [`JsCtx`] on [`Assets`].
    #[cfg(feature = "css")]
    css: CssCtx<'a>,
    #[cfg(feature = "js")]
    bundler: Option<&'a Js>,
}

/// The css handler's slice of the render context: the source asset root, used to
/// resolve `@import` ordering and `url()` keys relative to each sheet.
#[cfg(feature = "css")]
struct CssCtx<'a> {
    src: &'a Path,
}

impl Ctx<'_> {
    /// The served URL for a relative asset path, e.g. `/assets/css/app.css`.
    fn url(&self, rel: &Path) -> String {
        let rel = rel.to_string_lossy().replace('\\', "/");
        format!("{}/{rel}", self.prefix)
    }

    /// Lexically normalize a virtual asset path, collapsing `.`/`..` segments
    /// (the assets live under `dist`, so there is nothing to canonicalize).
    #[cfg(feature = "css")]
    fn normalize(path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
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
trait Handler {
    /// Whether this handler processes `file`. The first handler in [`builtin`]
    /// to claim a file owns it, so specific handlers come first and [`Verbatim`]
    /// claims whatever is left.
    fn claims(&self, file: &Path, config: &Config) -> bool;

    /// When this handler runs relative to the others.
    fn phase(&self) -> Phase {
        Phase::Early
    }

    /// Reorder this handler's files before rendering. The default keeps input
    /// order; stylesheets override it to fingerprint an imported sheet before
    /// its importer.
    fn order(&self, files: Vec<PathBuf>, _ctx: &Ctx) -> Vec<PathBuf> {
        files
    }

    /// Transform `file` (relative path `rel`) into the bytes written to `dist`,
    /// or `None` to emit nothing: a script partial pulled in only through
    /// imports. `map` holds the served names of every asset processed so far.
    fn render(&self, file: &Path, rel: &Path, map: &AssetMap, ctx: &Ctx)
    -> Result<Option<Vec<u8>>>;
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
    /// Source asset directory (`config.assets`).
    src: &'a Path,
    /// Destination directory under `dist`, named after `src` (e.g. `dist/assets`).
    dst: PathBuf,
    /// URL prefix the assets are served under, e.g. `/assets`.
    prefix: String,
}

impl<'a> Assets<'a> {
    pub fn new(config: &'a Config, #[cfg(feature = "js")] js: JsCtx<'a>) -> Self {
        Self {
            config,
            #[cfg(feature = "js")]
            js,
            src: &config.assets,
            dst: config.asset_dist(),
            prefix: format!("/{}", config.asset_name()),
        }
    }

    /// Process every asset into `dist`, returning the [`Processed`] summary.
    pub fn process(&self) -> Result<Processed> {
        let mut out = Processed::default();
        if !self.src.exists() {
            return Ok(out);
        }
        // Regenerate the whole tree so stale fingerprinted files never linger.
        if self.dst.exists() {
            fs::remove_dir_all(&self.dst)?;
        }
        let handlers = builtin();
        // Bucket every file under the first handler that claims it.
        let mut buckets: Vec<Vec<PathBuf>> = handlers.iter().map(|_| Vec::new()).collect();
        for file in Walk::files(self.src)? {
            let idx = handlers
                .iter()
                .position(|h| h.claims(&file, self.config))
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
                    Js::new(&cx)
                };
                let ctx = Ctx {
                    #[cfg(any(feature = "css", feature = "images"))]
                    config: self.config,
                    prefix: &self.prefix,
                    #[cfg(feature = "css")]
                    css: CssCtx { src: self.src },
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
            #[cfg(feature = "css")]
            css: CssCtx { src: self.src },
            #[cfg(feature = "js")]
            bundler: None,
        }
    }

    /// Render one handler's files (in its chosen order) and emit each result.
    fn run(
        &self,
        handler: &dyn Handler,
        files: Vec<PathBuf>,
        ctx: &Ctx,
        out: &mut Processed,
    ) -> Result<()> {
        for file in handler.order(files, ctx) {
            let rel = file
                .strip_prefix(self.src)
                .expect("Walk yields paths under src");
            if let Some(bytes) = handler.render(&file, rel, &out.map, ctx)? {
                self.emit(ctx, rel, &bytes, out)?;
            }
        }
        Ok(())
    }

    /// Fingerprint (when enabled) and write `bytes` for the asset at `rel`,
    /// recording the request->served URL mapping when the name changed.
    fn emit(&self, ctx: &Ctx, rel: &Path, bytes: &[u8], out: &mut Processed) -> Result<()> {
        let dst = self.fingerprint(rel, bytes);
        fs::write_all(self.dst.join(&dst), bytes)?;
        out.count += 1;
        out.bytes += bytes.len() as u64;
        if dst != rel {
            out.map.insert(ctx.url(rel), ctx.url(&dst));
        }
        Ok(())
    }

    /// The relative output path for an asset, splicing a content hash into the
    /// filename when fingerprinting is enabled (`app.css` -> `app.<hash>.css`).
    fn fingerprint(&self, rel: &Path, bytes: &[u8]) -> PathBuf {
        if !self.config.asset.fingerprint {
            return rel.to_path_buf();
        }
        let hash = Hash::of_bytes(bytes);
        let digest = &hash.hex()[..FINGERPRINT_LEN];
        let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let name = match rel.extension().and_then(|e| e.to_str()) {
            Some(ext) => format!("{stem}.{digest}.{ext}"),
            None => format!("{stem}.{digest}"),
        };
        rel.with_file_name(name)
    }
}

/// Recursively lists every file under a directory.
pub(super) struct Walk;

impl Walk {
    pub(super) fn files(root: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        Self::collect(root, &mut files)?;
        Ok(files)
    }

    fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for path in fs::read_dir(dir)? {
            if path.is_dir() {
                Self::collect(&path, out)?;
            } else {
                out.push(path);
            }
        }
        Ok(())
    }
}
