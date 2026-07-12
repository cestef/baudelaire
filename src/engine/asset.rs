//! The asset pipeline: minify, bundle, and fingerprint static assets.
//!
//! Runs before page compilation so the [`AssetMap`] it produces is available to
//! the render layer's fingerprint transform. Each asset is:
//!
//! - **CSS** — minified with lightningcss when `minify` is set, else copied.
//! - **JavaScript** — bundled (imports resolved, tree-shaken) and optionally
//!   minified with rolldown when `bundle` is set, else copied. Files whose name
//!   starts with `_` are treated as partials: pulled in through imports, never
//!   emitted standalone.
//! - **anything else** — copied verbatim.
//!
//! Every emitted file is content-hashed into its name when `fingerprint` is set,
//! and the original→final URL mapping recorded so references can be rewritten.

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use lightningcss::dependencies::{Dependency, DependencyOptions};
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet};
use rolldown::plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
    HookResolveIdReturn, HookUsage, Plugin, PluginContext, Pluginable, SharedLoadPluginContext,
};
use rolldown::{BundlerBuilder, BundlerOptions, InputItem, OutputFormat, RawMinifyOptions};
use rolldown_common::{ModuleType, Output};

use crate::config::{Config, ImageFormat, JpegConfig, PngConfig, PngStrip, SearchFormat};
use crate::error::{AssetError, Result};
use crate::fs;
use crate::graph::Hash;
use crate::render::AssetMap;

/// Length of the hex fingerprint spliced into asset filenames. 16 hex chars =
/// 64 bits of blake3 — collision-free in practice for a site's asset set.
const FINGERPRINT_LEN: usize = 16;

/// The outcome of processing the asset tree: the request→served URL map, the
/// count of files emitted, and their total byte size.
#[derive(Default)]
pub struct Processed {
    pub map: AssetMap,
    pub count: usize,
    pub bytes: u64,
}

/// The asset pipeline over one site's asset directory.
pub struct Assets<'a> {
    config: &'a Config,
    /// Source asset directory (`config.assets`).
    src: &'a Path,
    /// Destination directory under `dist`, named after `src` (e.g. `dist/assets`).
    dst: PathBuf,
    /// URL prefix the assets are served under, e.g. `/assets`.
    prefix: String,
}

impl<'a> Assets<'a> {
    pub fn new(config: &'a Config) -> Self {
        let src = &config.assets;
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("assets");
        Self {
            config,
            src,
            dst: config.dist.join(name),
            prefix: format!("/{name}"),
        }
    }

    /// Process every asset into `dist`, returning the map from original request
    /// URLs to the URLs they are actually served at (only entries renamed by
    /// fingerprinting appear), the count of files emitted (partials excluded),
    /// and their total byte size.
    pub fn process(&self) -> Result<Processed> {
        let mut out = Processed::default();
        if !self.src.exists() {
            return Ok(out);
        }
        // Regenerate the whole tree so stale fingerprinted files never linger.
        if self.dst.exists() {
            fs::remove_dir_all(&self.dst)?;
        }
        let bundler = self.config.asset.bundle.then(|| Js::new(self.config));
        // Two passes: every non-CSS asset is fingerprinted first so a stylesheet
        // can rewrite its `url()` references to the final (hashed) names. A CSS
        // file processed before its referenced image would otherwise emit a
        // `url()` pointing at a name that no longer exists in `dist`.
        let (css, other): (Vec<_>, Vec<_>) = Walk::files(self.src)?
            .into_iter()
            .partition(|file| Kind::of(file, self.config) == Kind::Css);
        for file in other {
            let rel = file
                .strip_prefix(self.src)
                .expect("Walk yields paths under src");
            let Some(bytes) = self.render(&file, Kind::of(&file, self.config), bundler.as_ref())?
            else {
                continue;
            };
            self.emit(rel, &bytes, &mut out)?;
        }
        for file in css {
            let rel = file
                .strip_prefix(self.src)
                .expect("Walk yields paths under src");
            let bytes = self.render_css(&file, rel, &out.map)?;
            self.emit(rel, &bytes, &mut out)?;
        }
        Ok(out)
    }

    /// Fingerprint (when enabled) and write `bytes` for the asset at `rel`,
    /// recording the request→served URL mapping when the name changed.
    fn emit(&self, rel: &Path, bytes: &[u8], out: &mut Processed) -> Result<()> {
        let dst = self.fingerprint(rel, bytes);
        self.write(&dst, bytes)?;
        out.count += 1;
        out.bytes += bytes.len() as u64;
        if dst != rel {
            out.map.insert(self.url(rel), self.url(&dst));
        }
        Ok(())
    }

    /// The processed bytes for one non-CSS asset, or `None` for a partial that
    /// is only pulled in through imports (never emitted standalone).
    fn render(&self, file: &Path, kind: Kind, bundler: Option<&Js>) -> Result<Option<Vec<u8>>> {
        let bytes = match kind {
            Kind::Module => return Ok(None),
            Kind::Entry => bundler
                .expect("bundler present when an entry is classified")
                .bundle(file)?,
            Kind::Image(format) => self.optimize(file, format)?,
            Kind::Css | Kind::Other => fs::read(file)?,
        };
        Ok(Some(bytes))
    }

    /// Process one stylesheet: minify (when enabled) and rewrite its `url()` /
    /// `@import` references to the fingerprinted names recorded in `map`, so a
    /// stylesheet still points at its assets after they are content-hashed.
    /// Copied verbatim when neither minify nor fingerprint is on.
    fn render_css(&self, file: &Path, rel: &Path, map: &AssetMap) -> Result<Vec<u8>> {
        if !self.config.asset.minify && !self.config.asset.fingerprint {
            return fs::read(file);
        }
        let code = fs::read_to_string(file)?;
        let mut sheet = StyleSheet::parse(&code, ParserOptions::default())
            .map_err(|e| AssetError::css(file.display(), e))?;
        if self.config.asset.minify {
            sheet
                .minify(MinifyOptions::default())
                .map_err(|e| AssetError::css(file.display(), e))?;
        }
        // only fingerprinting renames assets, so only then rewrite `url()`.
        let analyze = self.config.asset.fingerprint;
        let printed = sheet
            .to_css(PrinterOptions {
                minify: self.config.asset.minify,
                analyze_dependencies: analyze.then(DependencyOptions::default),
                ..PrinterOptions::default()
            })
            .map_err(|e| AssetError::css(file.display(), e))?;
        let mut out = printed.code;
        // lightningcss replaces each dependency URL with a placeholder; swap the
        // placeholder for the fingerprinted URL, or the original when unmapped.
        for dep in printed.dependencies.into_iter().flatten() {
            let (placeholder, url) = match dep {
                Dependency::Url(dep) => (dep.placeholder, dep.url),
                Dependency::Import(dep) => (dep.placeholder, dep.url),
            };
            let resolved = self.resolve_css_url(rel, &url, map).unwrap_or(url);
            out = out.replace(&placeholder, &resolved);
        }
        Ok(out.into_bytes())
    }

    /// The fingerprinted URL for a `url()` reference written in the stylesheet at
    /// `rel`, or `None` when it is external or unmapped. Relative references
    /// resolve against the stylesheet's own directory; absolute ones are already
    /// served URLs.
    fn resolve_css_url(&self, rel: &Path, raw: &str, map: &AssetMap) -> Option<String> {
        if raw.starts_with("data:")
            || raw.starts_with('#')
            || raw.starts_with("//")
            || raw.contains("://")
        {
            return None;
        }
        let key = if raw.starts_with('/') {
            raw.to_owned()
        } else {
            let dir = rel.parent().unwrap_or_else(|| Path::new(""));
            self.url(&Self::normalize(&dir.join(raw)))
        };
        map.resolve(&key)
    }

    /// Lexically normalize a virtual asset path, collapsing `.`/`..` segments
    /// (the assets live under `dist`, so there is nothing to canonicalize).
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

    /// Optimize one image, keeping the smaller of the original and the result:
    /// re-encoding can grow an already-tight file, and an optimizer must never
    /// make things worse.
    fn optimize(&self, file: &Path, format: ImageFormat) -> Result<Vec<u8>> {
        let bytes = fs::read(file)?;
        let optimize = &self.config.images.optimize;
        let optimized = match format {
            ImageFormat::Png => {
                Self::optimize_png(&bytes, optimize.png.as_ref().expect("png enabled"), file)?
            }
            ImageFormat::Jpeg => {
                Self::optimize_jpeg(&bytes, optimize.jpeg.as_ref().expect("jpeg enabled"), file)?
            }
        };
        Ok(if optimized.len() < bytes.len() {
            optimized
        } else {
            bytes
        })
    }

    /// Losslessly optimize a PNG with oxipng: recompress and strip chunks per the
    /// configured level and strip mode.
    fn optimize_png(bytes: &[u8], config: &PngConfig, file: &Path) -> Result<Vec<u8>> {
        let mut options = oxipng::Options::from_preset(config.level);
        options.strip = match config.strip {
            PngStrip::None => oxipng::StripChunks::None,
            PngStrip::Safe => oxipng::StripChunks::Safe,
            PngStrip::All => oxipng::StripChunks::All,
        };
        oxipng::optimize_from_memory(bytes, &options)
            .map_err(|e| AssetError::image(file.display(), e).into())
    }

    /// Re-encode a JPEG at the configured quality (lossy).
    fn optimize_jpeg(bytes: &[u8], config: &JpegConfig, file: &Path) -> Result<Vec<u8>> {
        let decoded =
            image::load_from_memory(bytes).map_err(|e| AssetError::image(file.display(), e))?;
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, config.quality)
            .encode_image(&decoded)
            .map_err(|e| AssetError::image(file.display(), e))?;
        Ok(out)
    }

    /// Write processed bytes to `rel` under the destination directory.
    fn write(&self, rel: &Path, bytes: &[u8]) -> Result<()> {
        fs::write_all(self.dst.join(rel), bytes)
    }

    /// The relative output path for an asset, splicing a content hash into the
    /// filename when fingerprinting is enabled (`app.css` → `app.<hash>.css`).
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

    /// The served URL for a relative asset path, e.g. `/assets/css/app.css`.
    fn url(&self, rel: &Path) -> String {
        let rel = rel.to_string_lossy().replace('\\', "/");
        format!("{}/{rel}", self.prefix)
    }
}

/// How an asset is processed, derived from its extension and the config. When
/// bundling is off, JavaScript is never classified as `Entry`/`Module` — it is
/// copied like any other file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A stylesheet (minified when enabled).
    Css,
    /// A JavaScript bundle entry point.
    Entry,
    /// A JavaScript partial (`_name.js`): imported only, not emitted standalone.
    Module,
    /// A raster image to optimize, in the given format.
    Image(ImageFormat),
    /// Any other file: copied verbatim.
    Other,
}

impl Kind {
    fn of(file: &Path, config: &Config) -> Self {
        let ext = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if ext.eq_ignore_ascii_case("css") {
            return Self::Css;
        }
        if config.asset.bundle && matches!(ext, "js" | "mjs" | "ts") {
            let partial = file
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('_'));
            return if partial { Self::Module } else { Self::Entry };
        }
        if let Some(format) = config.images.optimize.format(ext) {
            return Self::Image(format);
        }
        Self::Other
    }
}

/// A rolldown-backed JavaScript bundler. Owns a Tokio runtime to drive
/// rolldown's async build, reused across every entry in the site.
struct Js {
    runtime: tokio::runtime::Runtime,
    cwd: PathBuf,
    minify: bool,
    /// Serves the generated search client as `baudelaire:search` virtual modules
    /// so a user's own entry can import and bundle it.
    plugin: Arc<dyn Pluginable>,
}

impl Js {
    fn new(config: &Config) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        // Absolute so rolldown resolves entries regardless of its `cwd`.
        let cwd = fs::canonical(&config.assets);
        Self {
            runtime,
            cwd,
            minify: config.asset.minify,
            plugin: Arc::new(SearchModule::new(config)),
        }
    }

    /// Bundle a single entry to its output code.
    fn bundle(&self, entry: &Path) -> Result<Vec<u8>> {
        let import = fs::canonical(entry);
        let options = BundlerOptions {
            input: Some(vec![InputItem {
                name: None,
                import: import.to_string_lossy().into_owned(),
            }]),
            cwd: Some(self.cwd.clone()),
            format: Some(OutputFormat::Esm),
            minify: self.minify.then(|| RawMinifyOptions::Bool(true)),
            ..BundlerOptions::default()
        };
        let mut bundler = BundlerBuilder::default()
            .with_options(options)
            .with_plugins(vec![self.plugin.clone()])
            .build()
            .map_err(|e| AssetError::js(entry.display(), e))?;
        let output = self
            .runtime
            .block_on(bundler.generate())
            .map_err(|e| AssetError::js(entry.display(), e))?;
        output
            .assets
            .iter()
            .find_map(|out| match out {
                Output::Chunk(chunk) if chunk.is_entry => Some(chunk.code.clone().into_bytes()),
                _ => None,
            })
            .ok_or_else(|| AssetError::js(entry.display(), "no entry chunk produced").into())
    }
}

/// A rolldown plugin that serves baudelaire's generated search client as virtual
/// modules. A user's own bundle can `import { mountSearch } from
/// "baudelaire:search"` (or `.../json`, `.../inverted`) and have the palette
/// inlined, tree-shaken, minified, and fingerprinted like any first-party code.
#[derive(Debug)]
struct SearchModule {
    /// `baudelaire:search`: the format the site actually emits.
    default: String,
    /// `baudelaire:search/json`: the flat-index client.
    flat: String,
    /// `baudelaire:search/inverted`: the inverted-index client.
    inverted: String,
}

impl SearchModule {
    const DEFAULT: &'static str = "baudelaire:search";
    const FLAT: &'static str = "baudelaire:search/json";
    const INVERTED: &'static str = "baudelaire:search/inverted";

    fn new(config: &Config) -> Self {
        // The bare specifier follows the emitted index: inverted only when that
        // is the sole configured format, else the flat client (it has snippets).
        let default = if config.search.formats == [SearchFormat::Inverted] {
            SearchFormat::Inverted
        } else {
            SearchFormat::Json
        };
        Self {
            default: default.module(),
            flat: SearchFormat::Json.module(),
            inverted: SearchFormat::Inverted.module(),
        }
    }

    /// The module source for a virtual specifier, if it is one we serve.
    fn source(&self, id: &str) -> Option<&str> {
        match id {
            Self::DEFAULT => Some(&self.default),
            Self::FLAT => Some(&self.flat),
            Self::INVERTED => Some(&self.inverted),
            _ => None,
        }
    }
}

impl Plugin for SearchModule {
    fn name(&self) -> Cow<'static, str> {
        Self::DEFAULT.into()
    }

    fn resolve_id(
        &self,
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> impl std::future::Future<Output = HookResolveIdReturn> + Send {
        // claim our specifiers so rolldown skips filesystem resolution.
        let resolved = self
            .source(args.specifier)
            .map(|_| HookResolveIdOutput::from_id(args.specifier));
        async move { Ok(resolved) }
    }

    fn load(
        &self,
        _ctx: SharedLoadPluginContext,
        args: &HookLoadArgs<'_>,
    ) -> impl std::future::Future<Output = HookLoadReturn> + Send {
        let loaded = self.source(args.id).map(|code| HookLoadOutput {
            code: code.into(),
            module_type: Some(ModuleType::Js),
            ..HookLoadOutput::default()
        });
        async move { Ok(loaded) }
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load
    }
}

/// Recursively lists every file under a directory.
struct Walk;

impl Walk {
    fn files(root: &Path) -> Result<Vec<PathBuf>> {
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
