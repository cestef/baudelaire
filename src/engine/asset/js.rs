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

use crate::config::Config;
use crate::error::{AssetError, Result};
use crate::fs;
use crate::render::AssetMap;

use super::module::{ModuleCx, Virtual};
use super::{Ctx, Handler, PathExt, Phase};

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

    /// A bundle is JavaScript whatever its entry was written in.
    fn rename(&self, rel: &Path) -> PathBuf {
        rel.with_extension("js")
    }

    fn render(
        &self,
        file: &Path,
        _rel: &Path,
        _map: &AssetMap,
        ctx: &Ctx,
    ) -> Result<Option<Vec<u8>>> {
        let bundler = ctx.bundler.expect("bundler present when bundling");
        Ok(Some(bundler.bundle(file)?))
    }
}

/// A rolldown-backed JavaScript bundler. Owns a Tokio runtime to drive
/// rolldown's async build, reused across every entry in the site, and the
/// [`Virtual`] plugin that serves baudelaire's virtual modules.
pub(super) struct Js {
    runtime: tokio::runtime::Runtime,
    cwd: PathBuf,
    minify: bool,
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
            minify: cx.config.assets.minify,
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
    pub(super) fn bundle(&self, entry: &Path) -> Result<Vec<u8>> {
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
        // Code splitting is off, so exactly one chunk is expected. Anything
        // else would be silently discarded, so say so instead.
        let mut code = None;
        for asset in &output.assets {
            match asset {
                Output::Chunk(chunk) if chunk.is_entry && code.is_none() => {
                    code = Some(chunk.code.clone().into_bytes());
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
        code.ok_or_else(|| AssetError::js(entry.display(), "no entry chunk produced").into())
    }
}
