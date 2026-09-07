//! The handler protocol: what an asset kind is, and the registry of the kinds
//! this build knows. Adding a kind is one impl and one line in [`builtin`].

#[cfg(feature = "css")]
use std::path::Component;
use std::path::{Path, PathBuf};
#[cfg(feature = "sass")]
use std::sync::Arc;

use crate::config::{Config, SourceMaps};
use crate::error::Result;
use crate::fs;
use crate::graph::AssetName;
use crate::render::AssetMap;

use crate::engine::layers::Layered;

#[cfg(feature = "css")]
use super::css::Stylesheet;
#[cfg(feature = "images")]
use super::image::Raster;
#[cfg(feature = "js")]
use super::js::{Js, Script};

/// The ECMAScript-family extensions this build knows.
pub(super) struct Scripts;

impl Scripts {
    /// Every extension the bundler reads as a script, paired with whether a
    /// browser runs the file as written. Rolldown's own module-type table: an
    /// extension left out falls through to the verbatim copy, and one wrongly
    /// marked runnable publishes a source file the browser rejects.
    const TABLE: &'static [(&'static str, bool)] = &[
        ("js", true),
        ("mjs", true),
        ("cjs", true),
        ("jsx", false),
        ("ts", false),
        ("mts", false),
        ("cts", false),
        ("tsx", false),
    ];

    /// Whether the bundler reads this extension as a script. `ext` is compared
    /// as written, so callers lowercase first.
    #[cfg(feature = "js")]
    pub(super) fn known(ext: &str) -> bool {
        Self::runs(ext).is_some()
    }

    /// Whether this extension needs a build step to run at all, so publishing
    /// it unbundled serves a file the browser rejects.
    pub(super) fn unbundled(ext: &str) -> bool {
        Self::runs(ext) == Some(false)
    }

    fn runs(ext: &str) -> Option<bool> {
        Self::TABLE
            .iter()
            .find(|(name, _)| *name == ext)
            .map(|(_, runs)| *runs)
    }
}

/// What the pipeline reads but never publishes: the sources a build step
/// consumes, and the files a convention marks import-only. Only what this build
/// itself knows is listed; a file some other toolchain reads is copied like any
/// other, and the `_` convention is how a site keeps one out of the output.
pub(super) struct Private;

impl Private {
    /// Whether `rel` is an input rather than an artifact.
    pub(super) fn covers(rel: &Path, config: &Config) -> bool {
        let ext = rel.ext().to_ascii_lowercase();
        Self::partial(rel)
            || Self::declaration(rel)
            || Self::uncompiled(&ext)
            || (!config.assets.bundling() && Scripts::unbundled(&ext))
    }

    /// Whether `rel` is an input for one reason alone: bundling is off. The
    /// other exclusions a site reads off the filename; this one it cannot.
    pub(super) fn unbundled(rel: &Path, config: &Config) -> bool {
        !config.assets.bundling()
            && Scripts::unbundled(&rel.ext().to_ascii_lowercase())
            && !Self::partial(rel)
            && !Self::declaration(rel)
    }

    /// A Sass source in a binary with no Sass compiler: the one input this
    /// build recognizes and cannot turn into anything.
    fn uncompiled(ext: &str) -> bool {
        !cfg!(feature = "sass") && Config::SASS.contains(&ext)
    }

    /// A file whose name starts with `_` is imported by a neighbour, never
    /// served.
    fn partial(rel: &Path) -> bool {
        rel.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'))
    }

    /// A type declaration (`globals.d.ts`), read off the stem's own extension so
    /// `.d.mts` and `.d.cts` are the same rule.
    fn declaration(rel: &Path) -> bool {
        rel.file_stem()
            .map(Path::new)
            .and_then(Path::extension)
            .is_some_and(|e| e.eq_ignore_ascii_case("d"))
    }
}

/// When a handler runs, in order. `Early` assets (images, copies) provide the
/// fingerprinted names others reference; `Late` assets (stylesheets) rewrite
/// their references against them; `Bundle` assets (scripts) run last, so a
/// bundle importing `baudelaire:assets` sees the finalized map.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Early,
    Late,
    #[cfg(feature = "js")]
    Bundle,
}

/// The read-only context a handler renders against: the config, the served URL
/// prefix, and the shared JS bundler.
pub(super) struct Ctx<'a> {
    pub config: &'a Config,
    /// Preprocessed sources, by file: ordering a stylesheet reads it and so
    /// does transforming it, and for a Sass source reading it is a compile.
    #[cfg(feature = "sass")]
    pub compiled: Compiled,
    /// The asset roots as a search path, strongest first: where the Sass
    /// compiler resolves a `@use` that names no file it can see from the
    /// importing sheet.
    #[cfg(feature = "sass")]
    pub roots: Vec<PathBuf>,
    #[cfg(feature = "js")]
    pub bundler: Option<&'a Js>,
}

/// The sources this build has already preprocessed, keyed by file.
#[cfg(feature = "sass")]
#[derive(Default)]
pub(super) struct Compiled(parking_lot::Mutex<std::collections::HashMap<PathBuf, Arc<str>>>);

#[cfg(feature = "sass")]
impl Compiled {
    /// The preprocessed source for `file`, running `compile` the first time it
    /// is asked for.
    pub fn get(&self, file: &Path, compile: impl FnOnce() -> Result<String>) -> Result<Arc<str>> {
        if let Some(hit) = self.0.lock().get(file) {
            return Ok(Arc::clone(hit));
        }
        let text: Arc<str> = Arc::from(compile()?);
        self.0.lock().insert(file.to_owned(), Arc::clone(&text));
        Ok(text)
    }
}

impl Ctx<'_> {
    /// The served URL for a relative asset path, e.g. `/assets/css/app.css`.
    pub fn url(&self, rel: &Path) -> String {
        self.config.asset_url(rel)
    }

    /// Lexically normalize a virtual asset path, collapsing `.`/`..` segments.
    /// `None` when the path walks out of the asset root, which `PathBuf::pop`
    /// alone would silently absorb into a sibling that may really exist.
    #[cfg(feature = "css")]
    pub fn normalize(path: &Path) -> Option<PathBuf> {
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

/// Path knowledge the pipeline and its handlers share: how a file's kind is
/// read off its name, and how a suffix is spliced into that name.
pub(super) trait PathExt {
    /// The extension as written, or `""` when there is none; every caller
    /// lowercases before comparing.
    fn ext(&self) -> &str;

    /// The same path with `suffix` appended to the file stem, the extension
    /// kept: `photo.jpg` + `-480` -> `photo-480.jpg`.
    fn suffixed(&self, suffix: &str) -> PathBuf;
}

impl PathExt for Path {
    fn ext(&self) -> &str {
        self.extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
    }

    fn suffixed(&self, suffix: &str) -> PathBuf {
        AssetName::new(self, Some(suffix.to_owned())).path()
    }
}

/// One asset-processing strategy: which files it claims, when it runs, and how a
/// claimed file becomes its emitted bytes.
pub(super) trait Handler: Sync {
    /// What this handler is called in the debug log.
    fn name(&self) -> &'static str;

    /// Whether this handler processes `file`. The first handler in [`builtin`]
    /// to claim a file owns it, so specific handlers come first and [`Verbatim`]
    /// claims whatever is left.
    fn claims(&self, file: &Path, config: &Config) -> bool;

    /// When this handler runs relative to the others.
    fn phase(&self) -> Phase {
        Phase::Early
    }

    /// What becomes of the source map for the kind of asset this handler owns;
    /// [`SourceMaps::Off`] by default, so a kind that cannot produce a map says
    /// so by saying nothing.
    fn sourcemaps(&self, _config: &Config) -> SourceMaps {
        SourceMaps::Off
    }

    /// Whether this handler's output is a pure function of the file's own bytes
    /// and the config, and so can be memoized across builds. False by default:
    /// a stylesheet rewrites references to *other* assets' hashed names and a
    /// script bundles a whole import graph.
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
    fn rename(&self, rel: &Path) -> PathBuf {
        rel.to_path_buf()
    }

    /// Transform `file` (relative path `rel`) into what is written to `dist`.
    /// `map` holds the served names of every asset processed so far.
    fn render(&self, file: &Path, rel: &Path, map: &AssetMap, ctx: &Ctx) -> Result<Produced>;

    /// Responsive width variants derived from `file`, beyond the primary
    /// [`render`](Handler::render) output: the raster handler's downscaled
    /// copies. Default: none.
    fn variants(&self, _file: &Path, _rel: &Path, _ctx: &Ctx) -> Result<Vec<Variant>> {
        Ok(Vec::new())
    }
}

/// Everything a handler produced for one file: the bytes served under its own
/// name, and the source map they were built against, when it built one.
pub(in crate::engine) struct Produced {
    /// The served bytes, or `None` to emit nothing: a script partial pulled in
    /// only through imports.
    pub bytes: Option<Vec<u8>>,
    /// The source map for `bytes`, when the site asked for one and this handler
    /// can build it. Never carries the `sourceMappingURL` link: only the
    /// pipeline knows the fingerprinted name it has to point at.
    pub map: Option<Vec<u8>>,
}

impl Produced {
    pub(in crate::engine) fn bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes: Some(bytes),
            map: None,
        }
    }
}

/// One responsive candidate a handler derives from a source image: a target
/// `width`, its output path `rel`, and the `bytes` to write. `bytes` is `None`
/// for the source's own width, whose bytes are the handler's primary output; it
/// still becomes the largest `srcset` candidate.
pub(in crate::engine) struct Variant {
    pub rel: PathBuf,
    pub width: u32,
    pub bytes: Option<Vec<u8>>,
}

/// The registered handlers, in claim priority: [`Verbatim`] is last because it
/// claims every file. [`Script`] is present only under the `js` feature; without
/// it, `.js` files fall through to [`Verbatim`] and are copied unbundled.
pub(super) fn builtin() -> Vec<Box<dyn Handler>> {
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
    fn name(&self) -> &'static str {
        "verbatim"
    }

    fn claims(&self, _file: &Path, _config: &Config) -> bool {
        true
    }

    fn render(&self, file: &Path, _rel: &Path, _map: &AssetMap, _ctx: &Ctx) -> Result<Produced> {
        Ok(Produced::bytes(fs::read(file)?))
    }
}

#[cfg(test)]
mod tests {
    use super::PathExt;
    use std::path::{Path, PathBuf};

    #[cfg(feature = "css")]
    #[test]
    fn normalize_rejects_a_path_escaping_the_asset_root() {
        use super::Ctx;
        assert_eq!(Ctx::normalize(Path::new("../x.png")), None);
        assert_eq!(Ctx::normalize(Path::new("css/../../x.png")), None);
    }

    #[cfg(feature = "css")]
    #[test]
    fn normalize_collapses_interior_segments() {
        use super::Ctx;
        assert_eq!(
            Ctx::normalize(Path::new("css/../img/./logo.png")),
            Some(PathBuf::from("img/logo.png"))
        );
    }

    /// [`super::Verbatim`] claims every file, so anything registered after it
    /// could never run.
    #[test]
    fn only_the_last_handler_claims_a_file_nothing_else_wants() {
        let config = crate::config::Config::default();
        let handlers = super::builtin();
        let unknown = Path::new("data/notes.baudelaire-no-such-format");
        let claimed: Vec<usize> = handlers
            .iter()
            .enumerate()
            .filter(|(_, handler)| handler.claims(unknown, &config))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(claimed, [handlers.len() - 1]);
    }

    #[test]
    fn a_suffix_lands_between_the_stem_and_the_extension() {
        assert_eq!(
            Path::new("img/photo.jpg").suffixed("-480"),
            PathBuf::from("img/photo-480.jpg")
        );
    }
}
