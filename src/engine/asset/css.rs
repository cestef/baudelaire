//! The stylesheet handler: compile and minify with lightningcss, and rewrite
//! `url()` / `@import` references to the fingerprinted names of the assets they
//! point at.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lightningcss::dependencies::{Dependency, DependencyOptions};
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::targets::{Browsers, Targets};
use parcel_sourcemap::SourceMap;

use crate::config::{Config, SourceMaps, TargetConfig};
use crate::error::{AssetError, Result};
use crate::fs;
use crate::render::{AssetMap, Tail};

#[cfg(feature = "sass")]
use super::sass::Sass;
use super::{Ctx, Handler, PathExt, Phase, Produced};
use crate::engine::layers::Layered;

/// Stylesheets: minified when enabled, with their references rewritten to the
/// fingerprinted names recorded in the [`AssetMap`].
pub(super) struct Stylesheet;

impl Handler for Stylesheet {
    fn name(&self) -> &'static str {
        "stylesheet"
    }

    fn claims(&self, file: &Path, _config: &Config) -> bool {
        Self::claimed(file)
    }

    fn rename(&self, rel: &Path) -> PathBuf {
        Self::served(rel)
    }

    fn phase(&self) -> Phase {
        Phase::Late
    }

    fn sourcemaps(&self, config: &Config) -> SourceMaps {
        config.assets.sourcemap.styles
    }

    fn order(&self, files: Vec<Layered>, ctx: &Ctx) -> Vec<Layered> {
        Self::order(files, ctx)
    }

    fn render(&self, file: &Path, rel: &Path, map: &AssetMap, ctx: &Ctx) -> Result<Produced> {
        Self::transform(file, rel, map, ctx)
    }
}

/// The config's browser floor as lightningcss reads it, here rather than in
/// `config`, which must not name a type from a crate a feature can switch off.
impl From<&TargetConfig> for Browsers {
    fn from(targets: &TargetConfig) -> Self {
        let version = |v: Option<crate::config::Version>| v.map(|v| v.0);
        Self {
            android: version(targets.android),
            chrome: version(targets.chrome),
            edge: version(targets.edge),
            firefox: version(targets.firefox),
            ie: version(targets.ie),
            ios_saf: version(targets.ios),
            opera: version(targets.opera),
            safari: version(targets.safari),
            samsung: version(targets.samsung),
        }
    }
}

impl Stylesheet {
    /// The one test for "this is a stylesheet", used both to claim a file and to
    /// decide which of a sheet's references are sheets themselves.
    fn claimed(path: &Path) -> bool {
        #[cfg(feature = "sass")]
        if Sass::claims(path) {
            return true;
        }
        path.ext().eq_ignore_ascii_case("css")
    }

    /// The path a claimed file is served from; only a compiled source moves,
    /// since a browser reads a stylesheet by the MIME type its name earns.
    fn served(rel: &Path) -> PathBuf {
        #[cfg(feature = "sass")]
        if Sass::claims(rel) {
            return Sass::served(rel);
        }
        rel.to_path_buf()
    }

    /// The CSS text of a claimed file: compiled when it is a Sass source, read
    /// as it lies when it is already a stylesheet.
    #[cfg(feature = "sass")]
    fn source(file: &Path, ctx: &Ctx) -> Result<String> {
        if Sass::claims(file) {
            Sass::compile(file, ctx)
        } else {
            fs::read_to_string(file)
        }
    }

    #[cfg(not(feature = "sass"))]
    fn source(file: &Path, _ctx: &Ctx) -> Result<String> {
        fs::read_to_string(file)
    }

    /// Compile the sheet down to the site's browsers, minify it when enabled,
    /// and rewrite its references so it still points at its assets after they
    /// are content-hashed.
    fn transform(file: &Path, rel: &Path, map: &AssetMap, ctx: &Ctx) -> Result<Produced> {
        let assets = &ctx.config.assets;
        let wanted = assets.sourcemap.styles.wanted();
        let targets = Targets::from(Browsers::from(&assets.targets));
        let compile = assets.minify.css() || assets.targets.any();
        let preprocessed = rel != Self::served(rel);
        if !compile && !assets.fingerprint && !wanted && !preprocessed {
            return Ok(Produced::bytes(fs::read(file)?));
        }
        let code = Self::source(file, ctx)?;
        if !compile && !assets.fingerprint && !wanted {
            return Ok(Produced::bytes(code.into_bytes()));
        }
        let mut sheet = StyleSheet::parse(&code, ParserOptions::default())
            .map_err(|e| AssetError::css(file.display(), e))?;
        if compile {
            sheet
                .minify(MinifyOptions {
                    targets,
                    ..MinifyOptions::default()
                })
                .map_err(|e| AssetError::css(file.display(), e))?;
        }
        let mut sm = wanted.then(|| {
            let mut sm = SourceMap::new("/");
            let source = sm.add_source(&Self::served(rel).to_string_lossy());
            let _ = sm.set_source_content(source as usize, &code);
            sm
        });
        let analyze = assets.fingerprint;
        let printed = sheet
            .to_css(PrinterOptions {
                minify: assets.minify.css(),
                source_map: sm.as_mut(),
                analyze_dependencies: analyze.then(DependencyOptions::default),
                targets,
                ..PrinterOptions::default()
            })
            .map_err(|e| AssetError::css(file.display(), e))?;
        let mut out = printed.code;
        for dep in printed.dependencies.into_iter().flatten() {
            let (placeholder, url) = match dep {
                Dependency::Url(dep) => (dep.placeholder, dep.url),
                Dependency::Import(dep) => (dep.placeholder, dep.url),
            };
            let resolved = Self::resolve(rel, &url, map, ctx).unwrap_or(url);
            Self::swap(&mut out, &placeholder, &resolved, sm.as_mut());
        }
        let map = match sm.as_mut() {
            Some(sm) => Some(
                sm.to_json(None)
                    .map_err(|e| AssetError::css(file.display(), e))?
                    .into_bytes(),
            ),
            None => None,
        };
        Ok(Produced {
            bytes: Some(out.into_bytes()),
            map,
        })
    }

    /// Replace every `placeholder` in `out` with `resolved`, moving any source
    /// map along with the text: a resolved URL rarely has the placeholder's
    /// length, so every mapping later on the line refers to a moved column.
    fn swap(out: &mut String, placeholder: &str, resolved: &str, mut sm: Option<&mut SourceMap>) {
        let delta = i64::try_from(resolved.len()).unwrap_or(i64::MAX)
            - i64::try_from(placeholder.len()).unwrap_or(i64::MAX);
        let mut from = 0;
        while let Some(at) = out[from..].find(placeholder).map(|found| from + found) {
            if let Some(sm) = sm.as_deref_mut() {
                let (line, column) = Self::position(out, at);
                let _ = sm.offset_columns(line, column, delta);
            }
            out.replace_range(at..at + placeholder.len(), resolved);
            from = at + resolved.len();
        }
    }

    /// The zero-based line and column of byte offset `at` in `text`, counted the
    /// way a source map counts them.
    fn position(text: &str, at: usize) -> (u32, u32) {
        let before = &text[..at];
        let line = u32::try_from(before.matches('\n').count()).unwrap_or(u32::MAX);
        let start = before.rfind('\n').map_or(0, |nl| nl + 1);
        let column = u32::try_from(text[start..at].encode_utf16().count()).unwrap_or(u32::MAX);
        (line, column)
    }

    /// The fingerprinted URL for a reference written in the sheet at `rel`, or
    /// `None` when it is external or unmapped. The site's base path is prefixed
    /// here: the `BasePath` transform only walks the DOM, so a root-absolute
    /// URL emitted into CSS would 404 on a subpath-hosted site.
    fn resolve(rel: &Path, raw: &str, map: &AssetMap, ctx: &Ctx) -> Option<String> {
        let key = Self::key(rel, raw, ctx)?;
        let mapped = map.resolve(&key).url?;
        let mapped = ctx.config.prefixed(&mapped);
        Some(format!("{mapped}{}", Tail::of(raw).tail))
    }

    /// The asset-map key (served URL, no tail) for a reference written in the
    /// sheet at `rel`, or `None` when it is external. Relative references
    /// resolve against the sheet's own directory.
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
    /// the imported sheet's final name. Import cycles keep input order, and
    /// their cross-references fall back to the original names.
    fn order(files: Vec<Layered>, ctx: &Ctx) -> Vec<Layered> {
        if !ctx.config.assets.fingerprint || files.len() < 2 {
            return files;
        }
        let key_of = |file: &Layered| ctx.url(&Self::served(&file.rel));
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

    /// The asset-map keys of stylesheets referenced by `file`. Unreadable,
    /// uncompilable or unparseable input yields no deps: the error surfaces in
    /// `transform`.
    fn deps(file: &Layered, ctx: &Ctx) -> Vec<String> {
        let rel = file.rel.as_path();
        let Ok(code) = Self::source(&file.path, ctx) else {
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
                Self::claimed(Path::new(&key))
                    .then(|| Self::served(Path::new(&key)).to_string_lossy().into_owned())
            })
            .collect()
    }
}
