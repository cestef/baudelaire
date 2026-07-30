//! Inlines the SVG files that `@baudelaire/html`'s `svg()` marked.
//!
//! `svg()` emits an `<svg>` carrying the source path in one transient attribute
//! (see [`Html::MARKER`]). This transform reads that file, parses it, and
//! replaces the element's children with the file's own nodes, keeping the
//! caller's attributes and taking the file's root attributes as defaults.
//!
//! Inlining, rather than referencing, is the whole point: an `<img>` (and
//! typst's own `image()`) is an opaque replaced element, so an icon inside one
//! cannot inherit `currentColor`, cannot be recoloured by a theme toggle, and
//! cannot carry the caller's `class` or `aria-*`. Only real DOM can.
//!
//! The rule that HTML is never string-templated still holds: baudelaire parses
//! the file and builds typed nodes from it, so no markup is spliced as text and
//! a file cannot introduce anything the DOM could not express.
//!
//! **Paths are project-absolute.** `read()` cannot be used inside the module,
//! because a path in a package file resolves against the *package* root, not
//! the project. Baudelaire therefore reads the file itself, which also means
//! typst never sees it: each file read here is reported through
//! [`crate::render::Rewrite::read`] so the engine can add it to the page's
//! dependencies, or an edited icon would leave every page showing it a cache
//! hit.

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
/// HTML parsers do not require it, but it keeps the fragment valid when served
/// as XHTML or lifted out of the page, and it is what a hand-written icon
/// carries.
const XMLNS: &str = "http://www.w3.org/2000/svg";
const XMLNS_ATTR: HtmlAttr = HtmlAttr::constant("xmlns");

/// The one foreign namespace kept, with its prefix dropped rather than its
/// content: SVG 2 spells `xlink:href` as plain `href`, so an editor-written
/// `xlink:href` is carried across as the attribute it became.
const XLINK: &str = "http://www.w3.org/1999/xlink";

/// The attribute an icon carries when its stylesheet had to be confined to it,
/// and how many hex digits of the path hash identify it. Added only to a file
/// that actually has a `<style>`, so an ordinary icon gains nothing. Named
/// once: the CSS rewriter matches on the name and the element carries the
/// attribute, and the two have to be the same thing.
const SCOPE: &str = "data-svg";
const SCOPE_ATTR: HtmlAttr = HtmlAttr::constant(SCOPE);
const SCOPE_LEN: usize = 8;

/// The URL scheme an inlined file may never navigate to. Spelled once, so the
/// prefix length the check compares cannot drift from the scheme it names, nor
/// from the scheme the refusal reports.
const SCRIPT_SCHEME: &str = "javascript:";

/// How deep an icon may nest. Real drawings are a handful of levels; the cap is
/// here because [`Icon::children`] recurses, and a file deep enough to exhaust
/// the stack would abort the process rather than fail the build.
const DEPTH: usize = 128;

/// The marker attribute, interned once rather than per element. Its name is too
/// long for [`HtmlAttr::constant`]'s inline representation, but it is a
/// compile-time constant satisfying [`HtmlAttr::intern`]'s rules, so a failure
/// here is a bug in this file, never in a user's input.
static MARKER: LazyLock<HtmlAttr> =
    LazyLock::new(|| HtmlAttr::intern(Html::MARKER).expect("marker is a valid attribute name"));

/// The [`Transform`] that turns marked `<svg>` elements into inline DOM.
pub(super) struct Svg;

impl Transform for Svg {
    /// Always on: importing `svg()` is itself the opt-in, so there is no config
    /// block to gate this on, and a page that never calls it carries no marker
    /// for the walk to find.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let marker = *MARKER;
        let root = cx.root;
        // Findings are gathered during the walk and recorded after it: the walk
        // borrows the DOM mutably, so `cx` cannot be written inside it.
        let mut read = Vec::new();
        let mut failed = Vec::new();
        doc.walk(|element| {
            let Some(path) = element.attrs.get(marker).cloned() else {
                return;
            };
            element.attrs.0.retain(|(key, _)| *key != marker);
            match Self::inline(element, &path, root) {
                Ok(source) => read.push(source),
                Err(why) => failed.push(why),
            }
        });
        cx.found.read.extend(read);
        cx.found.invalid.extend(failed);
    }
}

impl Svg {
    /// Read the file `path` names, splice it into `element`, and return the
    /// file it read so the caller can record the dependency.
    ///
    /// The file's root attributes fill in under the caller's, so a template can
    /// override `width`, `fill` or `stroke` at the call site while everything
    /// it did not mention comes through as authored.
    fn inline(element: &mut HtmlElement, path: &str, root: &Path) -> Result<PathBuf, SvgError> {
        let source = Self::locate(path, root)?;
        let text =
            crate::fs::read_to_string(&source).map_err(|why| SvgError::unreadable(path, why))?;
        let parsed = roxmltree::Document::parse_with_options(&text, Icon::options())
            .map_err(|why| SvgError::malformed(path, why))?;
        let icon = Icon::root(&parsed, path)?;

        let mut attrs = HtmlAttrs::new();
        attrs.push(XMLNS_ATTR, XMLNS);
        icon.attributes(&mut attrs);
        for (key, value) in element.attrs.0.iter() {
            attrs.set(*key, value);
        }

        element.children = icon.children(0)?;
        element.attrs = attrs;
        Self::confine(element, path);
        Ok(source)
    }

    /// Confine any stylesheet the file carries to this icon.
    ///
    /// An inlined `<style>` is an ordinary page stylesheet, so an Illustrator
    /// export's `.st0{fill:#231F20}` would repaint every `.st0` on the page.
    /// Each rule is rewritten to match only inside this icon, and the icon is
    /// marked so that it can be. Keyed by path, so the same icon used twice
    /// scopes the same way and two different files can never collide.
    ///
    /// A file with no stylesheet gains no attribute and pays nothing.
    fn confine(element: &mut HtmlElement, path: &str) {
        let id = Hash::of_bytes(path.as_bytes()).short(SCOPE_LEN);
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

    /// The project file a marker names.
    ///
    /// Project-absolute, then [`Contained`]: the path comes from template text
    /// rather than from typst's own resolution, so it is the one place a marker
    /// could otherwise reach outside the project.
    fn locate(path: &str, root: &Path) -> Result<PathBuf, SvgError> {
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
    /// A DTD is allowed because Illustrator writes `<!DOCTYPE svg PUBLIC ..>`
    /// on essentially every export, and refusing it would reject a large share
    /// of real-world icons over a declaration that carries no content.
    /// roxmltree never resolves an *external* entity and caps internal
    /// expansion (depth 10, 255 references), so allowing the declaration opens
    /// no entity-expansion hole.
    fn options() -> roxmltree::ParsingOptions<'a> {
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..roxmltree::ParsingOptions::default()
        }
    }

    /// The document's root element, which must actually be an `<svg>`.
    ///
    /// Without this check a file that is not an SVG inlines as an empty
    /// `<svg>`: every child dropped for being in a foreign namespace, and
    /// nothing to say so.
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
    /// (`sodipodi:namedview`, or the `dc:title` inside an Inkscape
    /// `<metadata>`), which carries no meaning in a page and, stripped of its
    /// prefix, would collide with a real SVG tag.
    fn is_svg(&self) -> bool {
        matches!(self.node.tag_name().namespace(), None | Some(XMLNS))
    }

    /// Copy this element's attributes onto `attrs`, dropping those in a foreign
    /// namespace.
    ///
    /// roxmltree reports a name without its prefix, so `sodipodi:docname` would
    /// otherwise arrive as a plain `docname`: an editor's private bookkeeping
    /// silently promoted to an SVG attribute. Only the SVG namespace (the
    /// default, so unprefixed) and [`XLINK`] survive.
    ///
    /// An `xlink:href` never displaces a plain `href` written beside it, since
    /// SVG 2 makes the unprefixed one authoritative. [`Html::MARKER`] is
    /// skipped: a file carrying one would otherwise be re-read by the very walk
    /// placing these nodes, recursing until the stack gave out.
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

    /// Reject an element that would execute when the page loads it.
    ///
    /// Inlining makes an SVG part of the document, so anything active in the
    /// file runs with the page's origin. An icon pulled from a package or
    /// downloaded from an icon set is exactly where that is a surprise, so this
    /// is an error rather than a silent strip: a template that genuinely wants
    /// a script can write one where a reader will see it.
    fn active(&self) -> Result<(), SvgError> {
        if self.is_svg() && self.node.tag_name().name() == "script" {
            return Err(SvgError::active(self.path, "a `<script>`"));
        }
        for attribute in self.node.attributes() {
            let name = attribute.name();
            if name.len() > 2 && name[..2].eq_ignore_ascii_case("on") {
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
    /// A browser strips ASCII whitespace and control characters from a URL
    /// while parsing it, so `java&#9;script:` is `javascript:` by the time it
    /// is navigated. Comparing the raw text would miss exactly the spelling
    /// someone hiding a payload in an icon would reach for.
    fn executable(value: &str) -> bool {
        let squeezed: String = value
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && !c.is_control())
            .collect();
        squeezed
            .get(..SCRIPT_SCHEME.len())
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case(SCRIPT_SCHEME))
    }

    /// The typed nodes for this element's children.
    ///
    /// Comments, processing instructions and foreign-namespace elements are
    /// dropped, as is whitespace between elements: a pretty-printed icon would
    /// otherwise contribute a text node per line. Text with any non-whitespace
    /// is kept, so `<title>` and `<text>` survive intact.
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
        Svg::locate(path, root)
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
        // Relative, so there is no sane base to resolve it against; and every
        // shape of traversal, since the path is template text, not typst's own
        // resolution.
        // The root itself is refused with them: it used to resolve to the
        // project directory and fail later as an unreadable file.
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
        // A browser strips these before deciding the scheme, so this must too.
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
