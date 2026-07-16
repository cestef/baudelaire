//! The script handler and its rolldown-backed bundler: resolve imports,
//! tree-shake, minify, and serve baudelaire's `baudelaire:*` [`Virtual`]
//! modules (see [`super::module`]) into a user's entry.

use std::path::{Path, PathBuf};

use std::sync::Arc;

use rolldown::plugin::Pluginable;
use rolldown::{BundlerBuilder, BundlerOptions, InputItem, OutputFormat, RawMinifyOptions};
use rolldown_common::Output;

use crate::config::Config;
use crate::error::{AssetError, Result};
use crate::fs;
use crate::render::AssetMap;

use super::module::{ModuleCx, Virtual};
use super::{Ctx, Handler, PathExt, Phase};

/// JavaScript entries: bundled when `bundle` is on. A file whose name starts
/// with `_` is a partial: pulled in through imports, never emitted standalone.
/// With bundling off, JS is left to the verbatim copy.
///
/// Runs in [`Phase::Bundle`], the last phase, so a bundle importing
/// `baudelaire:assets` sees the finalized fingerprint map.
pub(super) struct Script;

impl Handler for Script {
    fn claims(&self, file: &Path, config: &Config) -> bool {
        config.asset.bundle
            && matches!(
                file.ext().to_ascii_lowercase().as_str(),
                "js" | "mjs" | "ts"
            )
    }

    fn phase(&self) -> Phase {
        Phase::Bundle
    }

    fn render(
        &self,
        file: &Path,
        _rel: &Path,
        _map: &AssetMap,
        ctx: &Ctx,
    ) -> Result<Option<Vec<u8>>> {
        if Self::partial(file) {
            return Ok(None);
        }
        let bundler = ctx.bundler.expect("bundler present when bundling");
        Ok(Some(bundler.bundle(file)?))
    }
}

impl Script {
    /// Whether `file` is an import-only partial (`_name.js`).
    fn partial(file: &Path) -> bool {
        file.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'))
    }
}

/// A rolldown-backed JavaScript bundler. Owns a Tokio runtime to drive
/// rolldown's async build, reused across every entry in the site, and the
/// [`Virtual`] plugin that serves baudelaire's virtual modules.
pub(super) struct Js {
    runtime: tokio::runtime::Runtime,
    cwd: PathBuf,
    minify: bool,
    plugin: Arc<dyn Pluginable>,
}

impl Js {
    /// Build the bundler against the finalized site context: call once the
    /// asset map is complete, so `baudelaire:assets` resolves every asset.
    pub(super) fn new(cx: &ModuleCx) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        // Absolute so rolldown resolves entries regardless of its `cwd`.
        let cwd = fs::canonical(&cx.config.assets);
        Self {
            runtime,
            cwd,
            minify: cx.config.asset.minify,
            plugin: Arc::new(Virtual::new(cx)),
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
