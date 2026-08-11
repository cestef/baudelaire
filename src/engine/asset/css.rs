//! The stylesheet handler: compile and minify with lightningcss, and rewrite
//! `url()` / `@import` references to the fingerprinted names of the assets they
//! point at.
//!
//! A Sass source is a stylesheet that needs one step in front of all that; see
//! [`Sass`](super::sass::Sass) for why it is a step here and not a handler of
//! its own.

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
/// fingerprinted names recorded in the [`AssetMap`]. Copied verbatim when
/// neither minify nor fingerprint is on.
pub(super) struct Stylesheet;

impl Handler for Stylesheet {
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

/// The config's browser floor as lightningcss reads it.
///
/// Here rather than in `config`, which must not name a type from a crate a
/// feature can switch off: the config states the versions, this states what
/// lightningcss calls them. `ios` is `ios_saf` there, which is the one name that
/// does not survive the trip unchanged.
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
    /// decide which of a sheet's references are sheets themselves: two spellings
    /// of it drifted apart the moment one grew a case or a suffix the other
    /// lacked.
    ///
    /// A Sass source is one of these, not a kind of its own: what it compiles to
    /// is a stylesheet, and everything downstream of the compile is what any
    /// other stylesheet gets.
    fn claimed(path: &Path) -> bool {
        #[cfg(feature = "sass")]
        if Sass::claims(path) {
            return true;
        }
        path.ext().eq_ignore_ascii_case("css")
    }

    /// The path a claimed file is served from. Only a compiled source moves:
    /// `app.scss` holds CSS once this handler is done with it, and a browser
    /// reads a stylesheet by the MIME type its name earns.
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
        match Sass::claims(file) {
            true => Sass::compile(file, ctx),
            false => fs::read_to_string(file),
        }
    }

    #[cfg(not(feature = "sass"))]
    fn source(file: &Path, _ctx: &Ctx) -> Result<String> {
        fs::read_to_string(file)
    }

    /// Compile the sheet down to the site's browsers, minify it when enabled, and
    /// rewrite its references so it still points at its assets after they are
    /// content-hashed. Copied verbatim when none of those is asked for.
    fn transform(file: &Path, rel: &Path, map: &AssetMap, ctx: &Ctx) -> Result<Produced> {
        let assets = &ctx.config.assets;
        let wanted = assets.sourcemap.styles.wanted();
        let targets = Targets::from(Browsers::from(&assets.targets));
        // Compiling for named browsers is a transform, not a minification: a
        // site may want its nesting flattened and its output still readable.
        let compile = assets.minify.css() || assets.targets.any();
        // A file that is already CSS and has nothing asked of it is bytes: read
        // and written without ever being text, so a sheet in some other encoding
        // survives a build that was not going to touch it anyway.
        let preprocessed = rel != Self::served(rel);
        if !compile && !assets.fingerprint && !wanted && !preprocessed {
            return Ok(Produced::bytes(fs::read(file)?));
        }
        let code = Self::source(file, ctx)?;
        // The same for a compiled source: nothing downstream wants it, so its
        // CSS goes out as the compiler wrote it.
        if !compile && !assets.fingerprint && !wanted {
            return Ok(Produced::bytes(code.into_bytes()));
        }
        let mut sheet = StyleSheet::parse(&code, ParserOptions::default())
            .map_err(|e| AssetError::css(file.display(), e))?;
        if compile {
            // The pass that downlevels: nesting, prefixes and colour fallbacks
            // are decided here, against the targets, whatever the printer does.
            sheet
                .minify(MinifyOptions {
                    targets,
                    ..MinifyOptions::default()
                })
                .map_err(|e| AssetError::css(file.display(), e))?;
        }
        // The sheet's own text travels inside the map, because the sheet itself
        // is not published under a name the map could point at: it is processed
        // into the file the map belongs to.
        let mut sm = wanted.then(|| {
            let mut sm = SourceMap::new("/");
            // Named for what the text *is*, which for a compiled source is the
            // CSS it became and not the Sass it was written as. grass emits no
            // map of its own, so there is nothing to chain back to the author's
            // file, and naming it would point every mapping at a line that says
            // something else.
            let source = sm.add_source(&Self::served(rel).to_string_lossy());
            let _ = sm.set_source_content(source as usize, &code);
            sm
        });
        // Only fingerprinting renames assets, so only then analyze `url()`s.
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
        // lightningcss replaces each dependency URL with a placeholder; swap it
        // for the fingerprinted URL, or the original when unmapped.
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
    /// map along with the text.
    ///
    /// The substitution happens *after* the map was recorded, and a resolved URL
    /// is almost never the length of the placeholder it replaces, so every
    /// mapping later on the same line refers to a column that has moved. Minified
    /// output is one long line, which makes that the whole file. Done as a plain
    /// `replace`, a stylesheet's map was accurate only until its first `url()`.
    fn swap(out: &mut String, placeholder: &str, resolved: &str, mut sm: Option<&mut SourceMap>) {
        // A stylesheet is a file that was read into memory, so neither length
        // is anywhere near the range where these conversions could lose one.
        let delta = i64::try_from(resolved.len()).unwrap_or(i64::MAX)
            - i64::try_from(placeholder.len()).unwrap_or(i64::MAX);
        let mut from = 0;
        while let Some(at) = out[from..].find(placeholder).map(|found| from + found) {
            if let Some(sm) = sm.as_deref_mut() {
                // Columns after the replacement move; the line does not, since
                // neither a placeholder nor a URL contains a newline.
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
        // UTF-16 units, which is what a source map's columns are.
        let column = u32::try_from(text[start..at].encode_utf16().count()).unwrap_or(u32::MAX);
        (line, column)
    }

    /// The fingerprinted URL for a reference written in the sheet at `rel`, or
    /// `None` when it is external or unmapped. Any `?query` / `#fragment` tail
    /// is preserved across the rewrite.
    fn resolve(rel: &Path, raw: &str, map: &AssetMap, ctx: &Ctx) -> Option<String> {
        let key = Self::key(rel, raw, ctx)?;
        // Only the URL: a stylesheet is not a page, so this lookup is nobody's
        // cache dependency. The sheet's own references are covered by hashing
        // its processed bytes.
        let mapped = map.resolve(&key).url?;
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
        if !ctx.config.assets.fingerprint || files.len() < 2 {
            return files;
        }
        // Keyed by what each sheet is *served* as, because that is the only name
        // another sheet can reference: nobody writes `@import "app.scss"` into
        // CSS, and a compiled source that keyed itself by its authored name
        // matched no importer and sorted as though it had none.
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
    ///
    /// A Sass source is compiled here as well as there, because what it
    /// references is decided by what it compiled *to*: a `@use` is resolved
    /// away by the compiler and is nobody's dependency, while the `url()` it
    /// emitted is one. Only a fingerprinting build ever asks.
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
                // Through `served` for the same reason `order` keys by it: a
                // reference written to the source name and one written to the
                // served name are the same edge, and only one of the two can be
                // the key.
                Self::claimed(Path::new(&key))
                    .then(|| Self::served(Path::new(&key)).to_string_lossy().into_owned())
            })
            .collect()
    }
}
