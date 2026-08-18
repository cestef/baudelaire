//! Inlines the SVG files that `@baudelaire/html`'s `svg()` marked.
//!
//! Paths are project-absolute, and every file read here is reported through
//! [`crate::render::Rewrite::read`] so the page depends on it.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use typst::ecow::EcoVec;
use typst::syntax::Span;
use typst_html::{HtmlAttr, HtmlAttrs, HtmlDocument, HtmlElement, HtmlNode, HtmlTag, tag};

use crate::config::Config;
use crate::error::SvgError;
use crate::fs::Contained;
use crate::graph::Hash;
use crate::render::scope::Scoped;
use crate::ui::markup;
use crate::world::module::Html;

use super::{AttrsExt, Cx, DocumentExt, ElementExt, Transform};

/// The namespace an inline `<svg>` declares, and the attribute declaring it.
const XMLNS: &str = "http://www.w3.org/2000/svg";
const XMLNS_ATTR: HtmlAttr = HtmlAttr::constant("xmlns");

/// The one foreign namespace kept, with its prefix dropped rather than its
/// content: SVG 2 spells `xlink:href` as plain `href`.
const XLINK: &str = "http://www.w3.org/1999/xlink";

/// The attribute an icon carries when its stylesheet had to be confined to it,
/// and how many hex digits of the path hash identify it.
const SCOPE: &str = "data-svg";
const SCOPE_ATTR: HtmlAttr = HtmlAttr::constant(SCOPE);
const SCOPE_LEN: usize = 8;

/// The URL scheme an inlined file may never navigate to.
const SCRIPT_SCHEME: &str = "javascript:";

/// How deep an icon may nest, capped because [`Icon::children`] recurses and a
/// deep enough file would exhaust the stack.
const DEPTH: usize = 128;

static MARKER: LazyLock<HtmlAttr> =
    LazyLock::new(|| HtmlAttr::intern(Html::MARKER).expect("marker is a valid attribute name"));

/// The ids an inlined icon defines, and the one rule for renaming them and
/// everything that points at them.
///
/// Keyed by the file's path, so the same icon used twice scopes the same way
/// and two files defining the same `id` never collide.
struct Ids<'a> {
    scope: &'a str,
    /// The ids this icon defines, longest first so a rewrite of `#ab` is never
    /// matched as `#a` followed by a stray `b`.
    names: Vec<String>,
}

impl<'a> Ids<'a> {
    /// The ids the file defines, which is every id on a *descendant*: the root
    /// by now also carries the caller's attributes, so an `id` there may be the
    /// page's and is not the file's to rename.
    fn of(element: &HtmlElement, scope: &'a str) -> Self {
        let mut names: Vec<String> = Vec::new();
        for child in &element.children {
            if let HtmlNode::Element(child) = child {
                child.visit(&mut |node| {
                    if let Some(id) = node.attrs.get(typst_html::attr::id) {
                        names.push(id.to_string());
                    }
                });
            }
        }
        names.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        names.dedup();
        Self { scope, names }
    }

    /// Rename every id the file defines, and rewrite every reference to one.
    ///
    /// References are found by value rather than by attribute name, since SVG
    /// points at an id from a dozen attributes; only a name this icon defines
    /// is rewritten, and the root's own `id` is the caller's.
    fn apply(&self, element: &mut HtmlElement) {
        if self.names.is_empty() {
            return;
        }
        for (key, value) in element.attrs.0.make_mut() {
            if *key != typst_html::attr::id {
                *value = self.referenced(value).into();
            }
        }
        for child in element.children.make_mut() {
            if let HtmlNode::Element(child) = child {
                child.walk(&mut |node| self.rewrite(node));
            }
        }
    }

    /// One descendant: its `id`, everything it points at, and a stylesheet it
    /// carries.
    fn rewrite(&self, node: &mut HtmlElement) {
        for (key, value) in node.attrs.0.make_mut() {
            *value = if *key == typst_html::attr::id {
                self.renamed(value).into()
            } else {
                self.referenced(value).into()
            };
        }
        if node.tag == tag::style {
            for child in node.children.make_mut() {
                if let HtmlNode::Text(css, _) = child {
                    *css = self.referenced(css).into();
                }
            }
        }
    }

    /// `name` under this icon's scope, or unchanged if the icon does not define
    /// it (an id typst or a template put there is not the file's to rename).
    fn renamed(&self, name: &str) -> String {
        if self.names.iter().any(|defined| defined == name) {
            format!("{name}-{}", self.scope)
        } else {
            name.to_owned()
        }
    }

    /// `value` with every `#name` naming one of this icon's ids renamed.
    ///
    /// A `#` followed by a name and then by anything that cannot continue an
    /// identifier, which covers `url(#a)`, `href="#a"` and a `#a { }` selector
    /// alike.
    fn referenced(&self, value: &str) -> String {
        let mut out = value.to_owned();
        for name in &self.names {
            let mut from = 0;
            while let Some(at) = out[from..].find(&format!("#{name}")) {
                let at = from + at;
                let after = at + 1 + name.len();
                let continues = out[after..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_');
                if continues {
                    from = after;
                    continue;
                }
                out.replace_range(after..after, &format!("-{}", self.scope));
                from = after + 1 + self.scope.len();
            }
        }
        out
    }
}

/// The [`Transform`] that turns marked `<svg>` elements into inline DOM.
pub(super) struct Svg;

impl Transform for Svg {
    /// Always on: importing `svg()` is itself the opt-in.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let marker = *MARKER;
        let root = cx.root;
        let sources = &cx.config.paths.sources;
        let mut read = Vec::new();
        let mut failed = Vec::new();
        doc.walk(|element| {
            let Some(path) = element.attrs.get(marker).cloned() else {
                return;
            };
            element.attrs.0.retain(|(key, _)| *key != marker);
            match Self::inline(element, &path, root, sources) {
                Ok(source) => read.push(source),
                Err(why) => failed.push(why),
            }
        });
        cx.found.read.extend(read);
        cx.found.invalid.extend(failed.into_iter().map(Into::into));
    }
}

impl Svg {
    /// Read the file `path` names, splice it into `element`, and return the
    /// file it read so the caller can record the dependency.
    ///
    /// The file's root attributes fill in under the caller's, which win.
    fn inline(
        element: &mut HtmlElement,
        path: &str,
        root: &Path,
        sources: &[(String, PathBuf)],
    ) -> Result<PathBuf, SvgError> {
        let source = Self::locate(path, root, sources)?;
        let text =
            crate::fs::read_to_string(&source).map_err(|why| SvgError::unreadable(path, why))?;
        let parsed = roxmltree::Document::parse_with_options(&text, Icon::options())
            .map_err(|why| SvgError::malformed(path, why))?;
        let icon = Icon::root(&parsed, path)?;

        let mut attrs = HtmlAttrs::new();
        attrs.push(XMLNS_ATTR, XMLNS);
        icon.attributes(&mut attrs);
        for (key, value) in &element.attrs.0 {
            attrs.set(*key, value);
        }

        element.children = icon.children(0)?;
        element.attrs = attrs;
        Self::confine(element, path);
        Ok(source)
    }

    /// Confine any stylesheet the file carries to this icon, which an inlined
    /// `<style>` needs since its rules would otherwise match the whole page.
    fn confine(element: &mut HtmlElement, path: &str) {
        let id = Hash::of_bytes(path.as_bytes()).short(SCOPE_LEN);
        Ids::of(element, &id).apply(element);
        let scoped = Scoped::attribute(SCOPE, &id);
        let mut found = false;
        element.walk(&mut |node| {
            if node.tag != tag::style {
                return;
            }
            for child in node.children.make_mut() {
                if let HtmlNode::Text(css, _) = child {
                    *css = scoped.stylesheet(css).into();
                    found = true;
                }
            }
        });
        if found {
            element.set(SCOPE_ATTR, &id);
        }
    }

    /// The project file a marker names: a declared source first, then
    /// project-absolute and [`Contained`], since the path comes from template
    /// text rather than typst's own resolution.
    fn locate(path: &str, root: &Path, sources: &[(String, PathBuf)]) -> Result<PathBuf, SvgError> {
        if let Some(file) = crate::world::module::Sources::real(path, sources, root) {
            return Ok(file);
        }
        let rel = path
            .strip_prefix('/')
            .and_then(Contained::new)
            .ok_or_else(|| SvgError::path(path))?;
        Ok(rel.under(root))
    }
}

/// One element of a parsed icon, carrying the path it came from so every
/// refusal names the file and not just the offending tag.
struct Icon<'a> {
    node: roxmltree::Node<'a, 'a>,
    path: &'a str,
}

impl<'a> Icon<'a> {
    /// How an icon file is parsed.
    ///
    /// A DTD is allowed because Illustrator writes one on every export;
    /// roxmltree never resolves an external entity and caps internal expansion,
    /// so it opens no entity-expansion hole.
    fn options() -> roxmltree::ParsingOptions<'a> {
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..roxmltree::ParsingOptions::default()
        }
    }

    /// The document's root element, which must actually be an `<svg>` or a
    /// foreign file would inline as an empty one.
    fn root(parsed: &'a roxmltree::Document<'a>, path: &'a str) -> Result<Self, SvgError> {
        let icon = Self {
            node: parsed.root_element(),
            path,
        };
        let name = icon.node.tag_name().name();
        if !icon.is_svg() || name != "svg" {
            return Err(SvgError::root(path, name));
        }
        icon.active()?;
        Ok(icon)
    }

    /// A view of another node in the same file.
    fn at(&self, node: roxmltree::Node<'a, 'a>) -> Self {
        Self {
            node,
            path: self.path,
        }
    }

    /// Whether this is SVG content rather than an editor's own bookkeeping
    /// (`sodipodi:namedview` and the like).
    fn is_svg(&self) -> bool {
        matches!(self.node.tag_name().namespace(), None | Some(XMLNS))
    }

    /// Copy this element's attributes onto `attrs`, keeping only the SVG
    /// namespace and [`XLINK`].
    ///
    /// An `xlink:href` never displaces a plain `href` beside it, and
    /// [`Html::MARKER`] is skipped so the walk placing these nodes cannot
    /// re-read the file forever.
    fn attributes(&self, attrs: &mut HtmlAttrs) {
        for attribute in self.node.attributes() {
            let prefixed = match attribute.namespace() {
                None => false,
                Some(XLINK) => true,
                Some(_) => continue,
            };
            if attribute.name() == Html::MARKER {
                continue;
            }
            let Ok(key) = HtmlAttr::intern(attribute.name()) else {
                continue;
            };
            if prefixed && attrs.get(key).is_some() {
                continue;
            }
            attrs.set(key, attribute.value());
        }
    }

    /// Reject an element that would execute when the page loads it, since
    /// inlining runs it with the page's origin.
    fn active(&self) -> Result<(), SvgError> {
        if self.is_svg() && self.node.tag_name().name() == "script" {
            return Err(SvgError::active(self.path, "a `<script>`"));
        }
        for attribute in self.node.attributes() {
            let name = attribute.name();
            if name.len() > 2
                && name
                    .as_bytes()
                    .get(..2)
                    .is_some_and(|head| head.eq_ignore_ascii_case(b"on"))
            {
                return Err(SvgError::active(
                    self.path,
                    markup!("an `{}` handler", name),
                ));
            }
            if Self::executable(attribute.value()) {
                return Err(SvgError::active(
                    self.path,
                    markup!("a `{}` {}", SCRIPT_SCHEME, name),
                ));
            }
        }
        Ok(())
    }

    /// Whether a URL runs script when followed.
    ///
    /// A browser strips ASCII whitespace and control characters while parsing a
    /// URL, so `java&#9;script:` navigates as `javascript:` and the raw text
    /// cannot be compared.
    fn executable(value: &str) -> bool {
        let squeezed: String = value
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && !c.is_control())
            .collect();
        squeezed
            .get(..SCRIPT_SCHEME.len())
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case(SCRIPT_SCHEME))
    }

    /// The typed nodes for this element's children, dropping comments,
    /// processing instructions, foreign-namespace elements and the whitespace
    /// between elements.
    fn children(&self, depth: usize) -> Result<EcoVec<HtmlNode>, SvgError> {
        if depth > DEPTH {
            return Err(SvgError::nested(self.path, DEPTH));
        }
        let mut out = EcoVec::new();
        for child in self.node.children() {
            if child.is_text() {
                let text = child.text().unwrap_or_default();
                if !text.trim().is_empty() {
                    out.push(HtmlNode::Text(text.into(), Span::detached()));
                }
                continue;
            }
            let child = self.at(child);
            if !child.node.is_element() || !child.is_svg() {
                continue;
            }
            child.active()?;
            let name = child.node.tag_name().name();
            let tag = HtmlTag::intern(name).map_err(|why| SvgError::tag(self.path, name, why))?;
            let mut element = HtmlElement::new(tag);
            child.attributes(&mut element.attrs);
            element.children = child.children(depth + 1)?;
            out.push(HtmlNode::Element(element));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{Icon, Svg};
    use std::path::{Path, PathBuf};

    fn locate(path: &str, root: &Path) -> Result<PathBuf, crate::error::SvgError> {
        Svg::locate(path, root, &[])
    }

    fn executable(url: &str) -> bool {
        Icon::executable(url)
    }

    #[test]
    fn locate_resolves_a_project_absolute_path() {
        let root = Path::new("/site");
        assert_eq!(
            locate("/assets/icons/x.svg", root).unwrap(),
            Path::new("/site/assets/icons/x.svg")
        );
    }

    #[test]
    fn locate_rejects_anything_that_could_leave_the_project() {
        let root = Path::new("/site");
        for path in [
            "assets/x.svg",
            "/../x.svg",
            "/assets/../../x.svg",
            "/./x.svg",
            "/",
            "",
        ] {
            assert!(locate(path, root).is_err(), "{path} should be rejected");
        }
    }

    #[test]
    fn executable_sees_through_url_whitespace() {
        for url in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "  javascript:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            "java\r\nscript:alert(1)",
            "j a v a s c r i p t : alert(1)",
        ] {
            assert!(executable(url), "{url:?} should be refused");
        }
    }

    #[test]
    fn executable_leaves_ordinary_urls_alone() {
        for url in [
            "#anchor",
            "/page/",
            "https://example.com",
            "url(#gradient)",
            "data:image/gif;base64,AAAA",
            "",
            "café",
        ] {
            assert!(!executable(url), "{url:?} should be allowed");
        }
    }
}
