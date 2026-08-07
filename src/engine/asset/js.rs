//! The script handler and its rolldown-backed bundler: resolve imports,
//! tree-shake, minify, and serve baudelaire's `baudelaire:*` [`Virtual`]
//! modules (see [`super::module`]) into a user's entry.

use std::path::{Path, PathBuf};

use std::sync::Arc;

use rolldown::plugin::Pluginable;
use rolldown::{BundlerBuilder, BundlerOptions, InputItem, OutputFormat, RawMinifyOptions};
use rolldown_common::CodeSplittingMode;
use rolldown_common::Output;
use rolldown_common::TsConfig;
use rolldown_common::{SourceMapType, StrOrBytes};

use crate::config::{Config, SourceMaps};
use crate::error::{AssetError, Result};
use crate::fs;
use crate::render::AssetMap;

use super::module::{ModuleCx, Virtual};
use super::{Ctx, Handler, PathExt, Phase, Produced};

/// Every extension the bundler reads as a script, which is rolldown's own
/// module-type table for the ECMAScript family. An extension left out of this
/// list is not "unsupported": it falls through to the verbatim copy, so a
/// `.tsx` entry used to ship its unstripped types to the browser.
const SCRIPTS: &[&str] = &["js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx"];

/// JavaScript entries: bundled when `bundle` is on. With bundling off, plain
/// JavaScript is left to the verbatim copy.
///
/// A partial (`_name.ts`) and a type declaration (`name.d.ts`) never reach a
/// handler at all: [`Private`](super::handler::Private) keeps the whole asset
/// tree's inputs out of the pipeline, whether or not this bundles them.
///
/// Runs in [`Phase::Bundle`], the last phase, so a bundle importing
/// `baudelaire:assets` sees the finalized fingerprint map.
pub(super) struct Script;

impl Handler for Script {
    fn claims(&self, file: &Path, config: &Config) -> bool {
        config.assets.bundle && SCRIPTS.contains(&file.ext().to_ascii_lowercase().as_str())
    }

    fn phase(&self) -> Phase {
        Phase::Bundle
    }

    fn sourcemaps(&self, config: &Config) -> SourceMaps {
        config.assets.sourcemap.scripts
    }

    /// A bundle is JavaScript whatever its entry was written in.
    fn rename(&self, rel: &Path) -> PathBuf {
        rel.with_extension("js")
    }

    fn render(&self, file: &Path, _rel: &Path, _map: &AssetMap, ctx: &Ctx) -> Result<Produced> {
        let bundler = ctx.bundler.expect("bundler present when bundling");
        bundler.bundle(file)
    }
}

/// A rolldown-backed JavaScript bundler. Owns a Tokio runtime to drive
/// rolldown's async build, reused across every entry in the site, and the
/// [`Virtual`] plugin that serves baudelaire's virtual modules.
pub(super) struct Js {
    runtime: tokio::runtime::Runtime,
    cwd: PathBuf,
    minify: bool,
    /// What becomes of a bundle's source map, which decides whether rolldown is
    /// asked for one at all.
    sourcemap: SourceMaps,
    /// The site's pinned `tsconfig.json`, absolute. `None` leaves rolldown on
    /// its own discovery, which walks up from each module as `tsc` does.
    tsconfig: Option<TsConfig>,
    plugin: Arc<dyn Pluginable>,
}

impl Js {
    /// Build the bundler against the finalized site context: call once the
    /// asset map is complete, so `baudelaire:assets` resolves every asset.
    /// Fallible: building the runtime spawns threads, which a constrained CI
    /// container refuses (EMFILE, `RLIMIT_NPROC`). That used to abort the whole
    /// build with a bare SIGABRT and no message, release builds being
    /// `panic = "abort"` and stripped.
    pub(super) fn new(cx: &ModuleCx) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(AssetError::runtime)?;
        // Absolute so rolldown resolves entries regardless of its `cwd`.
        let cwd = fs::canonical(&cx.config.paths.assets);
        Ok(Self {
            runtime,
            cwd,
            minify: cx.config.assets.minify.js,
            sourcemap: cx.config.assets.sourcemap.scripts,
            tsconfig: cx
                .config
                .assets
                .tsconfig
                .as_deref()
                .map(|path| Self::tsconfig(&cx.config.root, path))
                .transpose()?,
            plugin: Arc::new(Virtual::new(cx)),
        })
    }

    /// Pin the configured `tsconfig.json`, absolute. Resolved against the
    /// project root rather than left as written: the bundler's `cwd` is the
    /// asset directory, which is the one base a root-relative path is *not*
    /// written against.
    fn tsconfig(root: &Path, path: &Path) -> Result<TsConfig> {
        let full = root.join(path);
        match fs::canonicalize(&full) {
            Ok(full) => Ok(TsConfig::Manual(full)),
            Err(_) => Err(AssetError::tsconfig(path.display()).into()),
        }
    }

    /// Bundle a single entry to its output code.
    pub(super) fn bundle(&self, entry: &Path) -> Result<Produced> {
        let import = fs::canonical(entry);
        let options = BundlerOptions {
            input: Some(vec![InputItem {
                name: None,
                import: import.to_string_lossy().into_owned(),
            }]),
            cwd: Some(self.cwd.clone()),
            format: Some(OutputFormat::Esm),
            // One entry in, one file out. With splitting on, a dynamic
            // `import()` produced extra chunks the pipeline had no place to
            // write, so they were dropped and the emitted entry imported files
            // that did not exist in `dist`.
            code_splitting: Some(CodeSplittingMode::Bool(false)),
            minify: self.minify.then(|| RawMinifyOptions::Bool(true)),
            // `File`, so the map is its own file rather than a base64 blob
            // inside the bundle: the map is the larger of the two by far, and
            // inlining it would put it in front of every visitor instead of
            // only the one who opens devtools. The `sourceMappingURL` rolldown
            // would write names the file it thinks it is emitting, which is not
            // the fingerprinted name this pipeline writes, so it is stripped
            // and the pipeline writes the link itself.
            // Always `File`, whatever the posture: the pipeline decides where
            // the map ends up, because only it knows the fingerprinted name and
            // whether anything should point at it. `Inline` here would hand back
            // a bundle with the map already fused into it, past the point where
            // that choice can still be made.
            sourcemap: self.sourcemap.wanted().then_some(SourceMapType::File),
            tsconfig: self.tsconfig.clone(),
            ..BundlerOptions::default()
        };
        let mut bundler = BundlerBuilder::default()
            .with_options(options)
            .with_plugins(vec![Arc::clone(&self.plugin)])
            .build()
            .map_err(|e| AssetError::js(entry.display(), e))?;
        let output = self
            .runtime
            .block_on(bundler.generate())
            .map_err(|e| AssetError::js(entry.display(), e))?;
        // Code splitting is off, so exactly one chunk is expected, plus its map
        // when one was asked for. Anything else would be silently discarded, so
        // say so instead.
        let mut code = None;
        let mut map = None;
        for asset in &output.assets {
            match asset {
                Output::Chunk(chunk) if chunk.is_entry && code.is_none() => {
                    // Without the trailing `sourceMappingURL`: rolldown writes
                    // one naming the file it believes it is emitting, and the
                    // name this pipeline serves it under is not settled until it
                    // has been fingerprinted. The pipeline appends the real one.
                    code = Some(Self::unlinked(&chunk.code).into_bytes());
                    if let Some(chunk) = chunk.map.as_ref() {
                        map = Some(chunk.to_json_string().into_bytes());
                    }
                }
                Output::Asset(asset)
                    if self.sourcemap.wanted() && asset.filename.ends_with(".map") =>
                {
                    // A map rolldown chose to emit as a separate asset rather
                    // than hang off the chunk. Same file either way.
                    map.get_or_insert_with(|| match asset.source.clone() {
                        StrOrBytes::Str(text) => text.into_bytes(),
                        StrOrBytes::Bytes(bytes) => bytes,
                    });
                }
                other => {
                    let name = other.filename();
                    return Err(AssetError::js(
                        entry.display(),
                        format!(
                            "bundler produced an extra output this pipeline cannot emit: {name}"
                        ),
                    )
                    .into());
                }
            }
        }
        let bytes =
            code.ok_or_else(|| AssetError::js(entry.display(), "no entry chunk produced"))?;
        Ok(Produced {
            bytes: Some(bytes),
            map,
        })
    }

    /// `code` with any trailing `sourceMappingURL` comment removed.
    ///
    /// The bundler names the map after the filename it was told to write, which
    /// is not the fingerprinted one this pipeline serves. Left in place there
    /// would be two such comments, and a browser reads the last, so the wrong
    /// one would win exactly when fingerprinting is on.
    fn unlinked(code: &str) -> String {
        match code.rfind("//# sourceMappingURL=") {
            Some(at) => code[..at].to_owned(),
            None => code.to_owned(),
        }
    }
}
