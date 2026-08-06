//! The handler protocol: what an asset kind is, and the registry of the kinds
//! this build knows.
//!
//! One [`Handler`] owns one kind end to end: which files it claims, when it
//! runs, and how a claimed file becomes the bytes written to `dist`. Adding a
//! kind is a new impl and one line in [`builtin`]; the pipeline in
//! [`super::Assets`] never learns about it.

#[cfg(feature = "css")]
use std::path::Component;
use std::path::{Path, PathBuf};

use crate::config::Config;
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

/// What the pipeline reads but never publishes: the sources a build step
/// consumes, and the files a convention marks import-only.
///
/// The asset tree is both an input tree and an output tree, and nothing in a
/// file's extension says which. A `.scss` is a source, a `.ts` is a source
/// unless something bundles it, and `_partial.css` is a fragment its neighbour
/// imports. All three used to be copied verbatim to `dist`, so a site shipped
/// its own TypeScript (unrunnable in a browser, and carrying whatever the
/// comments said) beside the CSS its Sass produced.
///
/// One rule for the whole tree rather than a `claims` arm per handler: an
/// exclusion is not a kind, and the JS handler's own `_name` convention only
/// applied while bundling, which is exactly when the leak did not happen.
/// `static/` is untouched by this: that tree is the verbatim escape hatch, and a
/// host's `_redirects` has to publish under its own name.
pub(super) struct Private;

impl Private {
    /// Extensions no browser can use, whatever the config says: a preprocessor
    /// reads them and writes something else.
    const SOURCES: &'static [&'static str] = &["scss", "sass", "less", "styl"];

    /// Script sources that need a build step to run at all, so publishing one
    /// serves a file the browser rejects.
    const UNBUNDLED: &'static [&'static str] = &["ts", "mts", "cts", "tsx", "jsx"];

    /// Whether `rel` is an input rather than an artifact.
    pub(super) fn covers(rel: &Path, config: &Config) -> bool {
        let ext = rel.ext().to_ascii_lowercase();
        Self::partial(rel)
            || Self::declaration(rel)
            || Self::SOURCES.contains(&ext.as_str())
            || (!config.assets.bundle && Self::UNBUNDLED.contains(&ext.as_str()))
    }

    /// A file whose name starts with `_` is imported by a neighbour, never
    /// served: Sass has spelled partials this way for a decade, and the script
    /// handler already read it the same way.
    fn partial(rel: &Path) -> bool {
        rel.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'))
    }

    /// A type declaration (`globals.d.ts`), read off the stem's own extension so
    /// `.d.mts` and `.d.cts` are the same rule rather than two more cases.
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
/// prefix, and the shared JS bundler. The accumulating [`AssetMap`] is passed to
/// [`Handler::render`] separately, so the pipeline can keep mutating it between
/// calls.
pub(super) struct Ctx<'a> {
    /// The site config: read by the css and image handlers for their options,
    /// and by [`Ctx::url`], which is what keeps a served asset URL to one
    /// derivation whatever flavor this is.
    pub config: &'a Config,
    #[cfg(feature = "js")]
    pub bundler: Option<&'a Js>,
}

impl Ctx<'_> {
    /// The served URL for a relative asset path, e.g. `/assets/css/app.css`.
    pub fn url(&self, rel: &Path) -> String {
        self.config.asset_url(rel)
    }

    /// Lexically normalize a virtual asset path, collapsing `.`/`..` segments
    /// (the assets live under `dist`, so there is nothing to canonicalize).
    /// `None` when the path walks out of the asset root.
    ///
    /// Fallible because `PathBuf::pop` on an empty buffer is a silent no-op:
    /// `url(../x.png)` in `assets/a.css` normalized to `assets/x.png` and so
    /// resolved to a *different, real* file whenever one happened to exist.
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
    /// The lowercase-comparable extension, or `""` when there is none. Read by
    /// the css/js/image handlers to claim a file, and by [`Private`] to tell a
    /// build input from an artifact, which every flavor does.
    fn ext(&self) -> &str;

    /// The same path with `suffix` appended to the file stem, the extension
    /// kept: `photo.jpg` + `-480` -> `photo-480.jpg`. A responsive variant is
    /// the fingerprint splice with a suffix that is not a digest, so it goes
    /// through the same [`AssetName`] rule.
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
    /// Whether this handler processes `file`. The first handler in [`builtin`]
    /// to claim a file owns it, so specific handlers come first and [`Verbatim`]
    /// claims whatever is left.
    fn claims(&self, file: &Path, config: &Config) -> bool;

    /// When this handler runs relative to the others.
    fn phase(&self) -> Phase {
        Phase::Early
    }

    /// Whether this handler's output is a pure function of the file's own bytes
    /// and the config, and so can be memoized across builds.
    ///
    /// False by default, and deliberately so: a stylesheet rewrites references
    /// to *other* assets' hashed names and a script bundles a whole import
    /// graph, so neither is determined by the bytes in front of it.
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
    ///
    /// Scripts use it: a bundled `.ts` entry holds JavaScript, and writing it as
    /// `app.<hash>.ts` left the served file under a MIME type browsers refuse
    /// for `type=module`, keyed in the asset map under a name no author writes.
    fn rename(&self, rel: &Path) -> PathBuf {
        rel.to_path_buf()
    }

    /// Transform `file` (relative path `rel`) into the bytes written to `dist`,
    /// or `None` to emit nothing: a script partial pulled in only through
    /// imports. `map` holds the served names of every asset processed so far.
    fn render(&self, file: &Path, rel: &Path, map: &AssetMap, ctx: &Ctx)
    -> Result<Option<Vec<u8>>>;

    /// Responsive width variants derived from `file`, beyond the primary
    /// [`render`](Handler::render) output: the raster handler's downscaled
    /// copies. The pipeline writes each variant and records the `srcset`
    /// manifest from their widths. Default: none.
    ///
    /// [`Handler::render`]: Handler::render
    fn variants(&self, _file: &Path, _rel: &Path, _ctx: &Ctx) -> Result<Vec<Variant>> {
        Ok(Vec::new())
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

#[cfg(test)]
mod tests {
    use super::PathExt;
    use std::path::{Path, PathBuf};

    /// A `..` that walks out of the asset root must not be absorbed: it used to
    /// normalize to a sibling inside the root and resolve to a different, real
    /// file.
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

    /// The registry's one ordering invariant, which `builtin()` states in a
    /// comment and nothing enforced: [`super::Verbatim`] claims every file, so
    /// anything registered after it could never run. The crate's two other
    /// registries (`engine/emit/mod.rs`, `engine/compile/sidecar.rs`) both pin
    /// theirs; this one is why adding a handler is not quite "one impl plus one
    /// line".
    ///
    /// Holds in both flavors: with `css`, `js` and `images` off the fallback is
    /// the only handler there is, and it is still the last one.
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

    /// The trait is a spelling of [`crate::graph::AssetName`], which owns the
    /// splice and is tested there; this only pins the wiring.
    #[test]
    fn a_suffix_lands_between_the_stem_and_the_extension() {
        assert_eq!(
            Path::new("img/photo.jpg").suffixed("-480"),
            PathBuf::from("img/photo-480.jpg")
        );
    }
}
