//! The stylesheet handler: minify with lightningcss and rewrite `url()` /
//! `@import` references to the fingerprinted names of the assets they point at.

use std::collections::BTreeSet;
use std::path::Path;

use lightningcss::dependencies::{Dependency, DependencyOptions};
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet};

use crate::config::Config;
use crate::error::{AssetError, Result};
use crate::fs;
use crate::render::{AssetMap, Tail};

use super::{Ctx, Handler, PathExt, Phase};
use crate::engine::layers::Layered;

/// Stylesheets: minified when enabled, with their references rewritten to the
/// fingerprinted names recorded in the [`AssetMap`]. Copied verbatim when
/// neither minify nor fingerprint is on.
pub(super) struct Stylesheet;

impl Handler for Stylesheet {
    fn claims(&self, file: &Path, _config: &Config) -> bool {
        file.ext().eq_ignore_ascii_case("css")
    }

    fn phase(&self) -> Phase {
        Phase::Late
    }

    fn order(&self, files: Vec<Layered>, ctx: &Ctx) -> Vec<Layered> {
        Self::order(files, ctx)
    }

    fn render(
        &self,
        file: &Path,
        rel: &Path,
        map: &AssetMap,
        ctx: &Ctx,
    ) -> Result<Option<Vec<u8>>> {
        Self::transform(file, rel, map, ctx).map(Some)
    }
}

impl Stylesheet {
    /// Minify (when enabled) and rewrite the sheet's references so it still
    /// points at its assets after they are content-hashed. Copied verbatim when
    /// neither minify nor fingerprint is on.
    fn transform(file: &Path, rel: &Path, map: &AssetMap, ctx: &Ctx) -> Result<Vec<u8>> {
        if !ctx.config.asset.minify && !ctx.config.asset.fingerprint {
            return fs::read(file);
        }
        let code = fs::read_to_string(file)?;
        let mut sheet = StyleSheet::parse(&code, ParserOptions::default())
            .map_err(|e| AssetError::css(file.display(), e))?;
        if ctx.config.asset.minify {
            sheet
                .minify(MinifyOptions::default())
                .map_err(|e| AssetError::css(file.display(), e))?;
        }
        // Only fingerprinting renames assets, so only then analyze `url()`s.
        let analyze = ctx.config.asset.fingerprint;
        let printed = sheet
            .to_css(PrinterOptions {
                minify: ctx.config.asset.minify,
                analyze_dependencies: analyze.then(DependencyOptions::default),
                ..PrinterOptions::default()
            })
            .map_err(|e| AssetError::css(file.display(), e))?;
        let mut out = printed.code;
        // lightningcss replaces each dependency URL with a placeholder; swap it
        // for the fingerprinted URL, or the original when unmapped.
        for dep in printed.dependencies.into_iter().flatten() {
            let (placeholder, url) = match dep {
                Dependency::Url(dep) => (dep.placeholder, dep.url),
                Dependency::Import(dep) => (dep.placeholder, dep.url),
            };
            let resolved = Self::resolve(rel, &url, map, ctx).unwrap_or(url);
            out = out.replace(&placeholder, &resolved);
        }
        Ok(out.into_bytes())
    }

    /// The fingerprinted URL for a reference written in the sheet at `rel`, or
    /// `None` when it is external or unmapped. Any `?query` / `#fragment` tail
    /// is preserved across the rewrite.
    fn resolve(rel: &Path, raw: &str, map: &AssetMap, ctx: &Ctx) -> Option<String> {
        let key = Self::key(rel, raw, ctx)?;
        let mapped = map.resolve(&key)?;
        // Prefix the served base path here: the `BasePath` transform only walks
        // the DOM, so a root-absolute URL emitted into CSS would 404 on a
        // subpath-hosted site.
        let mapped = ctx.config.prefixed(&mapped);
        Some(format!("{mapped}{}", Tail::of(raw).tail))
    }

    /// The asset-map key (served URL, no tail) for a reference written in the
    /// sheet at `rel`, or `None` when it is external. Relative references
    /// resolve against the sheet's own directory; absolute ones are already
    /// served URLs.
    fn key(rel: &Path, raw: &str, ctx: &Ctx) -> Option<String> {
        if raw.starts_with("data:")
            || raw.starts_with('#')
            || raw.starts_with("//")
            || raw.contains("://")
        {
            return None;
        }
        let path = Tail::of(raw).path;
        Some(if path.starts_with('/') {
            path.to_owned()
        } else {
            let dir = rel.parent().unwrap_or_else(|| Path::new(""));
            ctx.url(&Ctx::normalize(&dir.join(path))?)
        })
    }

    /// Order stylesheets so a sheet referenced by another (`@import` /
    /// `url(*.css)`) is fingerprinted before its importer, whose rewrite needs
    /// the imported sheet's final name. Import cycles have no satisfying order;
    /// their members keep input order and cross-references fall back to the
    /// original (unmapped) names.
    fn order(files: Vec<Layered>, ctx: &Ctx) -> Vec<Layered> {
        if !ctx.config.asset.fingerprint || files.len() < 2 {
            return files;
        }
        let key_of = |file: &Layered| ctx.url(&file.rel);
        let all: BTreeSet<String> = files.iter().map(key_of).collect();
        let mut remaining: Vec<(Layered, Vec<String>)> = files
            .into_iter()
            .map(|file| {
                let deps = Self::deps(&file, ctx)
                    .into_iter()
                    .filter(|dep| all.contains(dep))
                    .collect();
                (file, deps)
            })
            .collect();
        let mut ordered = Vec::new();
        let mut done: BTreeSet<String> = BTreeSet::new();
        while !remaining.is_empty() {
            let (ready, rest): (Vec<_>, Vec<_>) = remaining
                .into_iter()
                .partition(|(_, deps)| deps.iter().all(|dep| done.contains(dep)));
            if ready.is_empty() {
                ordered.extend(rest.into_iter().map(|(file, _)| file));
                break;
            }
            for (file, _) in ready {
                done.insert(key_of(&file));
                ordered.push(file);
            }
            remaining = rest;
        }
        ordered
    }

    /// The asset-map keys of stylesheets referenced by `file`. Unreadable or
    /// unparseable input yields no deps: the error surfaces in `transform`.
    fn deps(file: &Layered, ctx: &Ctx) -> Vec<String> {
        let rel = file.rel.as_path();
        let Ok(code) = fs::read_to_string(&file.path) else {
            return Vec::new();
        };
        let Ok(sheet) = StyleSheet::parse(&code, ParserOptions::default()) else {
            return Vec::new();
        };
        let Ok(printed) = sheet.to_css(PrinterOptions {
            analyze_dependencies: Some(DependencyOptions::default()),
            ..PrinterOptions::default()
        }) else {
            return Vec::new();
        };
        printed
            .dependencies
            .into_iter()
            .flatten()
            .filter_map(|dep| {
                let url = match dep {
                    Dependency::Url(dep) => dep.url,
                    Dependency::Import(dep) => dep.url,
                };
                let key = Self::key(rel, &url, ctx)?;
                key.to_ascii_lowercase().ends_with(".css").then_some(key)
            })
            .collect()
    }
}
