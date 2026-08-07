//! The `@baudelaire/*` modules themselves: the two whose source is generated
//! into memory with the world. The file-backed pair (`sections`, `pages`) has
//! no impl here: their source is a table the build writes, served by path.

use crate::codegen::Value;
use crate::world::BuildContext;

use super::{Module, ModuleCx};

/// `@baudelaire/html`: element construction without `html.elem`'s ceremony, and
/// `svg()`, which inlines an SVG file as real DOM.
///
/// Everything but `svg()` is pure Typst that a template could have written
/// itself; it ships here so every site gets it. `svg()` leaves the markers
/// below for [`crate::render`] to resolve.
pub struct Html;

impl Html {
    /// The transient attribute `svg()` leaves on the element, naming the file
    /// to inline. Removed when [`crate::render`] splices the file in, so it
    /// never reaches the output. Bound into the module source rather than
    /// written into `typ/html.typ`, so the name lives in one place across both
    /// sides of the marker.
    pub(crate) const MARKER: &'static str = "data-baudelaire-svg";

    /// The binding `typ/html.typ` reads the marker through.
    const MARKER_BINDING: &'static str = "_svg-marker";
}

impl Module for Html {
    fn name(&self) -> &'static str {
        "html"
    }

    fn bindings(&self, _cx: &ModuleCx) -> Vec<(String, Value)> {
        vec![(Self::MARKER_BINDING.to_owned(), Value::str(Self::MARKER))]
    }

    fn body(&self) -> &'static str {
        include_str!("typ/html.typ")
    }
}

/// `@baudelaire/site`: site identity and build version as typed bindings, so a
/// template writes `#import "@baudelaire/site": title` instead of
/// `sys.inputs.at("baudelaire", default: (:)).at("site", ..).at("title", ..)`.
/// A typo becomes an import error rather than a silent `none`.
///
/// The field list belongs to [`BuildContext::site_fields`], which its JavaScript
/// counterpart `baudelaire:site` serves too, so the two cannot drift.
///
/// Config-derived values only. `git` and `date` stay on `sys.inputs`; see the
/// module-level note on why nothing volatile may be baked in.
pub(super) struct Site;

/// `@baudelaire/markdown`: `md()`, which renders a chunk of markdown inside a
/// Typst page.
///
/// The only module whose body is not pure Typst underneath: `md()` calls a
/// native function (see [`crate::world::markdown`]) because lowering markdown
/// is Rust. What this module contributes is the site's own parser settings,
/// which that function cannot capture, and the argument handling around them.
#[cfg(feature = "markdown")]
pub(super) struct Markdown;

#[cfg(feature = "markdown")]
impl Module for Markdown {
    fn name(&self) -> &'static str {
        "markdown"
    }

    fn bindings(&self, cx: &ModuleCx) -> Vec<(String, Value)> {
        use crate::config::Named;
        let markdown = cx.markdown;
        vec![
            // The name of the global the body looks up, so the two sides of the
            // lookup are one constant rather than two spellings.
            (
                "_binding".to_owned(),
                Value::str(crate::world::markdown::INTERNAL),
            ),
            (
                "_extensions".to_owned(),
                Value::Array(
                    markdown
                        .extensions
                        .iter()
                        .map(|extension| Value::str(extension.name()))
                        .collect(),
                ),
            ),
            ("_html".to_owned(), Value::str(markdown.html.name())),
            ("_eval".to_owned(), Value::Bool(markdown.eval)),
        ]
    }

    fn body(&self) -> &'static str {
        include_str!("typ/markdown.typ")
    }
}

impl Module for Site {
    fn name(&self) -> &'static str {
        "site"
    }

    fn bindings(&self, cx: &ModuleCx) -> Vec<(String, Value)> {
        BuildContext::site_fields(cx.context)
    }

    fn body(&self) -> &'static str {
        include_str!("typ/site.typ")
    }
}
