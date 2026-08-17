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
/// module-type table for the ECMAScript family; one left out falls through to
/// the verbatim copy.
const SCRIPTS: &[&str] = &["js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx"];

/// JavaScript entries: bundled when `bundle` is on, left to the verbatim copy
/// otherwise. Runs in [`Phase::Bundle`], the last phase, so a bundle importing
/// `baudelaire:assets` sees the finalized fingerprint map.
pub(super) struct Script;

impl Handler for Script {
    fn claims(&self, file: &Path, config: &Config) -> bool {
        config.assets.bundling() && SCRIPTS.contains(&file.ext().to_ascii_lowercase().as_str())
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

/// A rolldown-backed JavaScript bundler, owning the Tokio runtime that drives
/// rolldown's async build and the [`Virtual`] plugin serving the virtual
/// modules.
pub(super) struct Js {
    runtime: tokio::runtime::Runtime,
    cwd: PathBuf,
    minify: bool,
    sourcemap: SourceMaps,
    /// The site's pinned `tsconfig.json`, absolute. `None` leaves rolldown on
    /// its own discovery, which walks up from each module as `tsc` does.
    tsconfig: Option<TsConfig>,
    plugin: Arc<dyn Pluginable>,
}

impl Js {
    /// Build the bundler against the finalized site context: call once the
    /// asset map is complete, so `baudelaire:assets` resolves every asset.
    /// Fallible because the runtime spawns threads, which a constrained
    /// container refuses.
    pub(super) fn new(cx: &ModuleCx) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(AssetError::runtime)?;
        let cwd = fs::canonical(&cx.config.paths.assets);
        Ok(Self {
            runtime,
            cwd,
            minify: cx.config.assets.minify.js(),
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
    /// project root, not the bundler's `cwd`, which is the asset directory.
    fn tsconfig(root: &Path, path: &Path) -> Result<TsConfig> {
        let full = root.join(path);
        fs::canonicalize(&full).map_or_else(
            |_| Err(AssetError::tsconfig(path.display()).into()),
            |full| Ok(TsConfig::Manual(full)),
        )
    }

    /// Bundle a single entry to its output code: one entry in, one file out,
    /// since the pipeline has nowhere to write the extra chunks code splitting
    /// would produce.
    pub(super) fn bundle(&self, entry: &Path) -> Result<Produced> {
        let import = fs::canonical(entry);
        let options = BundlerOptions {
            input: Some(vec![InputItem {
                name: None,
                import: import.to_string_lossy().into_owned(),
            }]),
            cwd: Some(self.cwd.clone()),
            format: Some(OutputFormat::Esm),
            code_splitting: Some(CodeSplittingMode::Bool(false)),
            minify: self.minify.then(|| RawMinifyOptions::Bool(true)),
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
        let mut code = None;
        let mut map = None;
        for asset in &output.assets {
            match asset {
                Output::Chunk(chunk) if chunk.is_entry && code.is_none() => {
                    code = Some(Self::unlinked(&chunk.code).into_bytes());
                    if let Some(chunk) = chunk.map.as_ref() {
                        map = Some(chunk.to_json_string().into_bytes());
                    }
                }
                Output::Asset(asset)
                    if self.sourcemap.wanted() && asset.filename.ends_with(".map") =>
                {
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

    /// `code` with any trailing `sourceMappingURL` comment removed: the bundler
    /// names the map after the file it was told to write, and the pipeline
    /// appends a link to the fingerprinted name it actually serves.
    fn unlinked(code: &str) -> String {
        code.rfind("//# sourceMappingURL=")
            .map_or_else(|| code.to_owned(), |at| code[..at].to_owned())
    }
}
