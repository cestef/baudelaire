//! The generated modules whose source is generated into memory; the
//! file-backed pair (`sections`, `pages`) is a table served by path.

use crate::codegen::Value;
use crate::world::BuildContext;

use super::{Module, ModuleCx};

/// `/.baudelaire/site/html.typ`: element construction without `html.elem`'s ceremony, and
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

    /// The transient attribute on the wrapper `fence()` puts around a code
    /// block, whose other attributes [`crate::render`] moves onto the block's
    /// `<pre>` before dropping the wrapper.
    pub(crate) const FENCE: &'static str = "data-baudelaire-fence";

    /// The binding `typ/html.typ` reads that marker through.
    const FENCE_BINDING: &'static str = "_fence-marker";
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
            (Self::FENCE_BINDING.to_owned(), Value::str(Self::FENCE)),
        ]
    }

    fn body(&self) -> &'static str {
        include_str!("typ/html.typ")
    }
}

/// `/.baudelaire/site/toc.typ`: a table of contents, filled in from the page's own
/// headings once it has compiled.
pub struct Toc;

impl Toc {
    /// The transient attribute `toc()` leaves on the element, holding the
    /// heading levels to list as `<from>-<to>`, removed when [`crate::render`]
    /// fills the element in.
    pub(crate) const MARKER: &'static str = "data-baudelaire-toc";

    /// The transient attribute naming the list element to build, `ol` or `ul`.
    pub(crate) const LIST: &'static str = "data-baudelaire-toc-list";

    /// The bindings `typ/toc.typ` writes those two through.
    const MARKER_BINDING: &'static str = "_toc-marker";
    const LIST_BINDING: &'static str = "_toc-list";
}

impl Module for Toc {
    fn name(&self) -> &'static str {
        "toc"
    }

    fn bindings(&self, _cx: &ModuleCx) -> Vec<(String, Value)> {
        vec![
            (Self::MARKER_BINDING.to_owned(), Value::str(Self::MARKER)),
            (Self::LIST_BINDING.to_owned(), Value::str(Self::LIST)),
        ]
    }

    fn body(&self) -> &'static str {
        include_str!("typ/toc.typ")
    }
}

/// `/.baudelaire/site/site.typ`: site identity and build version as typed bindings, so a
/// template writes `#import "/.baudelaire/site/site.typ": title` and a typo becomes an
/// import error rather than a silent `none`.
///
/// Config-derived values only: nothing volatile may be baked in, so `git` and
/// `date` stay on `sys.inputs`.
pub(super) struct Site;

/// `/.baudelaire/site/sources.typ`: the files `paths { sources }` declared, each bound to
/// the path it is served under.
///
/// A page writes a *name* and never a path, so content can reach a declared
/// file and nothing else.
///
/// ```typ
/// #import "/.baudelaire/site/sources.typ": notes
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

/// `/.baudelaire/site/markdown.typ`: `md()`, which renders a chunk of markdown inside a
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
