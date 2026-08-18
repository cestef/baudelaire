//! The `@baudelaire/*` modules whose source is generated into memory; the
//! file-backed pair (`sections`, `pages`) is a table served by path.

use crate::codegen::Value;
use crate::world::BuildContext;

use super::{Module, ModuleCx};

/// `@baudelaire/html`: element construction without `html.elem`'s ceremony, and
/// `svg()`, which inlines an SVG file as real DOM.
pub struct Html;

impl Html {
    /// The transient attribute `svg()` leaves on the element, naming the file
    /// to inline, removed when [`crate::render`] splices that file in.
    pub(crate) const MARKER: &'static str = "data-baudelaire-svg";

    /// The binding `typ/html.typ` reads the marker through.
    const MARKER_BINDING: &'static str = "_svg-marker";

    /// The binding `typ/html.typ` writes the lint marker through, so the
    /// attribute is spelled once, in [`crate::render::lint`].
    const LINT_BINDING: &'static str = "_lint-marker";
}

impl Module for Html {
    fn name(&self) -> &'static str {
        "html"
    }

    fn bindings(&self, _cx: &ModuleCx) -> Vec<(String, Value)> {
        vec![
            (Self::MARKER_BINDING.to_owned(), Value::str(Self::MARKER)),
            (
                Self::LINT_BINDING.to_owned(),
                Value::str(crate::render::lint::EXEMPT.resolve()),
            ),
        ]
    }

    fn body(&self) -> &'static str {
        include_str!("typ/html.typ")
    }
}

/// `@baudelaire/site`: site identity and build version as typed bindings, so a
/// template writes `#import "@baudelaire/site": title` and a typo becomes an
/// import error rather than a silent `none`.
///
/// Config-derived values only: nothing volatile may be baked in, so `git` and
/// `date` stay on `sys.inputs`.
pub(super) struct Site;

/// `@baudelaire/sources`: the files `paths { sources }` declared, each bound to
/// the path it is served under.
///
/// A page writes a *name* and never a path, so content can reach a declared
/// file and nothing else.
///
/// ```typ
/// #import "@baudelaire/sources": notes
/// #include notes            // a `.typ` source is a body
/// #json(notes)              // any other kind is data
/// ```
pub(super) struct Sources;

impl Module for Sources {
    fn name(&self) -> &'static str {
        "sources"
    }

    fn bindings(&self, cx: &ModuleCx) -> Vec<(String, Value)> {
        cx.sources
            .iter()
            .map(|(name, path)| (name.clone(), Value::str(super::Sources::vpath(name, path))))
            .collect()
    }

    /// Nothing hand-written: the bindings are the module.
    fn body(&self) -> &'static str {
        ""
    }
}

/// `@baudelaire/markdown`: `md()`, which renders a chunk of markdown inside a
/// Typst page.
///
/// The body wraps a native function (see [`crate::world::markdown`]) with the
/// site's own parser settings, which that function cannot capture.
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
