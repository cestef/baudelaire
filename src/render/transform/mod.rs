//! Per-page transforms over the typed HTML DOM, applied before serialization.
//!
//! [`Transforms::builtin`] is the single source of the pipeline: a new pass is
//! one `impl Transform` plus one line in that list. What order it runs in is
//! [`Transform::after`], not that line: the list is sorted by it.

mod anchors;
mod base;
mod embed;
mod exempt;
mod externalize;
mod fences;
mod fingerprint;
mod footnotes;
mod highlight;
mod image;
mod integrity;
mod lang;
mod math;
mod meta;
mod outbound;
mod rewrite;
mod sheets;
mod sources;
mod spans;
mod speculation;
#[cfg(feature = "announce")]
mod standard;
mod svg;
mod toc;

pub use externalize::ImageRef;

use std::sync::LazyLock;

use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, HtmlTag, attr, tag};

use crate::config::Config;
use crate::content::Page;

use super::AssetMap;
use super::LinkMap;
use super::SrcSets;
use anchors::Anchors;
use base::BasePath;
use embed::Embed;
use exempt::Exempt;
use externalize::Externalize;
use fences::Fences;
use fingerprint::Fingerprint;
use footnotes::Footnotes;
use highlight::Highlight;
use image::Images;
use integrity::Integrity;
use lang::Lang;
use math::Math;
use meta::Meta;
use outbound::Outbound;
use rewrite::Links;
use sheets::Sheets;
use sources::Sources;
use spans::Spans;
use speculation::Speculation;
#[cfg(feature = "announce")]
use standard::Verify;
use svg::Svg;
use toc::Toc;

/// Per-page context handed to every transform. Transforms run sequentially for
/// a page, so they share this one mutable accumulator.
pub(super) struct Cx<'a> {
    pub config: &'a Config,
    pub page: &'a Page,
    /// What the page set makes of this page: the pages it sits between, its
    /// editions in other languages.
    pub related: &'a crate::content::Related,
    pub entities: &'a crate::content::Registries,
    pub links: &'a LinkMap,
    pub assets: &'a AssetMap,
    pub srcsets: &'a SrcSets,
    /// What this build wrote and what each file digests to.
    pub emitted: &'a super::Emitted,
    pub root: &'a std::path::Path,
    /// The content tree as the compiler spells it: what a link's origin is
    /// tested against to tell an author's own reference from a layout's chrome.
    pub content: &'a std::path::Path,
    pub world: &'a crate::world::PageWorld,
    /// What the pipeline has found so far, and the value the caller gets back.
    pub found: super::Rewrite,
    /// The width variants this page's *extracted* images will be given, keyed
    /// by the URL each is served at.
    ///
    /// Written by [`Externalize`] and read by [`Sources`], which declares
    /// [`Transform::after`] on it.
    pub extracted: std::collections::BTreeMap<String, Vec<super::Candidate>>,
    /// Every code fence on the page, as [`Fences`] gathered it: written here
    /// rather than read off the DOM, since that pass is what removes the hidden
    /// lines the snippet lint has to check.
    pub fences: Vec<super::snippet::Snippet>,
    /// What the author has kept the lint off, as [`Exempt`] read it before
    /// removing the markers that said so. Every other pass declares
    /// [`Transform::after`] on it, since the spans it records are the ones
    /// typst produced.
    pub exempt: super::lint::Exemptions,
}

/// The attributes that unconditionally carry a URL to an asset this site owns.
///
/// `srcset` and `content` are conditional, and handled by
/// [`ElementExt::assets`].
const URL_ATTRS: &[HtmlAttr] = &[attr::href, attr::src, attr::poster];

/// SVG's root tag, which typst-html's HTML-only `tag` module does not name.
static SVG: LazyLock<HtmlTag> =
    LazyLock::new(|| HtmlTag::intern("svg").expect("svg is a valid tag name"));

/// The attribute OpenGraph names its tags with, where HTML uses `name`.
pub(super) const PROPERTY: HtmlAttr = HtmlAttr::constant("property");

/// The `<meta>` keys whose `content` is a URL; every other one is prose, and
/// rewriting it as a URL corrupts titles and descriptions.
const URL_META: &[&str] = &[
    "og:image",
    "og:image:url",
    "og:image:secure_url",
    "og:url",
    "twitter:image",
];

/// The heading elements in level order, so an index into this *is* the heading
/// level.
const HEADINGS: &[HtmlTag] = &[tag::h1, tag::h2, tag::h3, tag::h4, tag::h5, tag::h6];

/// The replace-or-push rule for an attribute list, for a pass still assembling
/// attributes that has no element to hang them off yet.
pub(super) trait AttrsExt {
    /// Set `key` to `value`, replacing an existing entry rather than appending
    /// a duplicate (which is invalid HTML).
    fn set(&mut self, key: HtmlAttr, value: &str);
    /// Drop `key` entirely, which is not the same as emptying it to `key=""`.
    fn remove(&mut self, key: HtmlAttr);
}

impl AttrsExt for typst_html::HtmlAttrs {
    fn set(&mut self, key: HtmlAttr, value: &str) {
        match self.get_mut(key) {
            Some(existing) => *existing = value.into(),
            None => self.push(key, value),
        }
    }

    fn remove(&mut self, key: HtmlAttr) {
        self.0.retain(|(k, _)| *k != key);
    }
}

pub(super) trait ElementExt {
    /// Visit this element, then every descendant element, depth-first.
    fn walk(&mut self, f: &mut impl FnMut(&mut HtmlElement));
    /// The same walk, read-only: what a pass that only *looks* at the DOM
    /// takes, since [`walk`](ElementExt::walk) clones each shared child list.
    fn visit(&self, f: &mut impl FnMut(&HtmlElement));
    /// The text this element and its descendants carry, markup dropped, **in
    /// document order**: what a heading's anchor is slugged from, and the bytes
    /// an inline `<script>` or `<style>` ships.
    ///
    /// A [`silent`] descendant or an inlined `<svg>` contributes nothing, but
    /// the element the call is made *on* is never skipped.
    ///
    /// [`silent`]: ElementExt::silent
    fn text(&self) -> String;
    /// Whether this element carries no text a reader reads: a script or style
    /// body, or something declaring itself hidden from assistive technology.
    fn silent(&self) -> bool;
    /// This element's heading level, `1`..`6`, or `None` for anything that is
    /// not a heading.
    fn heading(&self) -> Option<u8>;
    /// Whether this element pulls in a stylesheet, by token rather than string
    /// comparison: `rel` may hold several (`rel="preload stylesheet"`).
    fn stylesheet(&self) -> bool;
    fn head(&mut self) -> Option<&mut HtmlElement>;
    /// Set `key` to `value`, replacing an existing attribute or appending one.
    fn set(&mut self, key: HtmlAttr, value: &str);
    /// Rewrite each attribute among `keys` that is present: `f` returns the
    /// replacement value, or `None` to leave it as authored.
    fn rewrite(&mut self, keys: &[HtmlAttr], f: impl FnMut(&str) -> Option<String>);
    /// Rewrite every asset-bearing attribute ([`URL_ATTRS`] plus `srcset`)
    /// through `f` in one pass, so an asset-rewriting transform names no key
    /// list of its own.
    fn assets(&mut self, f: impl FnMut(&str) -> Option<String>);
}

impl ElementExt for HtmlElement {
    fn walk(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        for child in self.children.make_mut() {
            if let HtmlNode::Element(child) = child {
                child.walk(f);
            }
        }
    }

    fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        for child in &self.children {
            if let HtmlNode::Element(child) = child {
                child.visit(f);
            }
        }
    }

    fn text(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<&HtmlNode> = self.children.iter().rev().collect();
        while let Some(node) = stack.pop() {
            match node {
                HtmlNode::Text(text, _) => out.push_str(text),
                HtmlNode::Element(child) if child.silent() || child.tag == *SVG => {}
                HtmlNode::Element(child) => stack.extend(child.children.iter().rev()),
                _ => {}
            }
        }
        out
    }

    fn silent(&self) -> bool {
        self.tag == tag::script
            || self.tag == tag::style
            || self
                .attrs
                .get(attr::aria_hidden)
                .is_some_and(|hidden| hidden == "true")
    }

    fn heading(&self) -> Option<u8> {
        HEADINGS
            .iter()
            .position(|&heading| heading == self.tag)
            .and_then(|level| u8::try_from(level + 1).ok())
    }

    fn stylesheet(&self) -> bool {
        self.tag == tag::link
            && self.attrs.get(attr::rel).is_some_and(|rel| {
                rel.split_ascii_whitespace()
                    .any(|token| token.eq_ignore_ascii_case("stylesheet"))
            })
    }

    fn head(&mut self) -> Option<&mut HtmlElement> {
        self.children
            .make_mut()
            .iter_mut()
            .find_map(|node| match node {
                HtmlNode::Element(el) if el.tag == tag::head => Some(el),
                _ => None,
            })
    }

    fn set(&mut self, key: HtmlAttr, value: &str) {
        self.attrs.set(key, value);
    }

    fn rewrite(&mut self, keys: &[HtmlAttr], mut f: impl FnMut(&str) -> Option<String>) {
        for &key in keys {
            if let Some(value) = self.attrs.get_mut(key)
                && let Some(new) = f(value)
            {
                *value = new.into();
            }
        }
    }

    fn assets(&mut self, mut f: impl FnMut(&str) -> Option<String>) {
        self.rewrite(URL_ATTRS, &mut f);
        if self.tag == tag::meta
            && [PROPERTY, attr::name]
                .iter()
                .filter_map(|&key| self.attrs.get(key))
                .any(|key| URL_META.contains(&key.as_str()))
        {
            self.rewrite(&[attr::content], &mut f);
        }
        let Some(value) = self.attrs.get_mut(attr::srcset) else {
            return;
        };
        if let Some(rebuilt) = SrcSet(value.as_str()).rewritten(&mut f) {
            *value = rebuilt.into();
        }
    }
}

/// [`ElementExt`] rooted at the document.
pub(super) trait DocumentExt {
    fn walk(&mut self, f: impl FnMut(&mut HtmlElement));
    fn visit(&self, f: impl FnMut(&HtmlElement));
    fn head(&mut self) -> Option<&mut HtmlElement>;
    fn assets(&mut self, f: impl FnMut(&str) -> Option<String>);
}

impl DocumentExt for HtmlDocument {
    fn walk(&mut self, mut f: impl FnMut(&mut HtmlElement)) {
        self.root_mut().walk(&mut f);
    }

    fn visit(&self, mut f: impl FnMut(&HtmlElement)) {
        self.root().visit(&mut f);
    }

    fn head(&mut self) -> Option<&mut HtmlElement> {
        self.root_mut().head()
    }

    fn assets(&mut self, mut f: impl FnMut(&str) -> Option<String>) {
        self.walk(|element| element.assets(&mut f));
    }
}

/// A `srcset` attribute value: a comma-separated list of `url [descriptor]`
/// candidates, so `<img srcset>` and `<source srcset>` get the same asset
/// rewriting as a plain `src`.
struct SrcSet<'a>(&'a str);

impl<'a> SrcSet<'a> {
    /// This list with each URL passed through `f`, descriptors left intact, or
    /// `None` when `f` replaced nothing.
    fn rewritten(&self, mut f: impl FnMut(&str) -> Option<String>) -> Option<String> {
        let mut changed = false;
        let rebuilt = self
            .candidates()
            .into_iter()
            .map(|(url, descriptor)| {
                let url = f(url).map_or_else(
                    || url.to_owned(),
                    |new| {
                        changed = true;
                        new
                    },
                );
                match descriptor {
                    "" => url,
                    d => format!("{url} {d}"),
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        changed.then_some(rebuilt)
    }

    /// The `(url, descriptor)` candidates, by the HTML spec's whitespace-driven
    /// rule: a URL runs to the next whitespace and may itself contain commas
    /// (`data:` URIs), so only a URL's *trailing* commas (or a comma after the
    /// descriptor) terminate a candidate.
    fn candidates(&self) -> Vec<(&'a str, &'a str)> {
        let mut out = Vec::new();
        let mut rest = self.0;
        loop {
            rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
            if rest.is_empty() {
                break;
            }
            let split = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let (url, tail) = rest.split_at(split);
            let trimmed = url.trim_end_matches(',');
            if trimmed.len() != url.len() {
                out.push((trimmed, ""));
                rest = tail;
                continue;
            }
            let (descriptor, after) = tail
                .find(',')
                .map_or((tail, ""), |i| (&tail[..i], &tail[i + 1..]));
            out.push((url, descriptor.trim()));
            rest = after;
        }
        out
    }
}

/// A per-page pass over the typed HTML DOM. `Send + Sync` because the owning
/// [`super::Renderer`] is shared read-only across the parallel compile pool.
pub(super) trait Transform: Send + Sync {
    /// How [`Transform::after`] names this pass. Its own `NAME` const, so a
    /// dependency is spelled once and a typo is a compile error.
    fn name(&self) -> &'static str;

    /// The passes that must have run before this one.
    ///
    /// Required rather than defaulted: where a pass sits is a decision, and the
    /// hand-written list in [`Transforms::builtin`] cannot be one, since
    /// reordering it produces silently wrong markup that no other test sees.
    /// [`Transforms::builtin`] sorts by this, so the list is a preference among
    /// passes nothing separates and never the contract itself.
    fn after(&self) -> &'static [&'static str];

    /// Whether to run, from config alone.
    fn enabled(&self, config: &Config) -> bool;
    /// Rewrite `doc` in place, optionally recording findings in `cx`.
    /// Best-effort: a transform that cannot act on a node leaves it untouched.
    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>);
}

/// The built-in transforms, in apply order.
pub(super) struct Transforms(Vec<Box<dyn Transform>>);

impl Transforms {
    /// The pipeline in apply order: resolve links, restructure and annotate
    /// authored markup, synthesize `<head>` elements, add responsive srcsets,
    /// inline embeds, fingerprint whatever references remain, shift them under
    /// the base path, and digest the finished markup last.
    pub(super) fn builtin() -> Self {
        Self(Self::ordered(Self::declared()))
    }

    /// The passes as written, before [`Transforms::ordered`] settles them.
    fn declared() -> Vec<Box<dyn Transform>> {
        vec![
            Box::new(Exempt),
            Box::new(Links),
            Box::new(Svg),
            Box::new(Lang),
            Box::new(Anchors),
            Box::new(Footnotes),
            Box::new(Toc),
            Box::new(Highlight),
            Box::new(Fences),
            Box::new(Spans),
            Box::new(Meta),
            Box::new(Math),
            Box::new(Sheets),
            Box::new(Speculation),
            Box::new(Outbound),
            #[cfg(feature = "announce")]
            Box::new(Verify),
            Box::new(Images),
            Box::new(Externalize),
            Box::new(Sources),
            Box::new(Embed),
            Box::new(Fingerprint),
            Box::new(BasePath),
            Box::new(Integrity),
        ]
    }

    /// `passes` with each one moved after everything its [`Transform::after`]
    /// names, and otherwise left where it was written.
    ///
    /// Stable, so the written order is what separates two passes no constraint
    /// does. A pass naming one that is not in the pipeline never becomes
    /// placeable and is caught here rather than by wrong markup.
    fn ordered(passes: Vec<Box<dyn Transform>>) -> Vec<Box<dyn Transform>> {
        let mut waiting: Vec<Option<Box<dyn Transform>>> = passes.into_iter().map(Some).collect();
        let mut placed: Vec<&'static str> = Vec::with_capacity(waiting.len());
        let mut out = Vec::with_capacity(waiting.len());
        while out.len() < waiting.len() {
            let next = waiting
                .iter()
                .position(|slot| {
                    slot.as_ref()
                        .is_some_and(|pass| pass.after().iter().all(|name| placed.contains(name)))
                })
                .expect("every transform's dependencies are in the pipeline, and acyclic");
            let pass = waiting[next].take().expect("just found");
            placed.push(pass.name());
            out.push(pass);
        }
        out
    }

    /// The pipeline's passes, in the order they will be applied.
    #[cfg(test)]
    fn names(&self) -> Vec<&'static str> {
        self.0.iter().map(|pass| pass.name()).collect()
    }

    /// Apply every enabled transform to `doc`, in order.
    pub(super) fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        for transform in &self.0 {
            if transform.enabled(cx.config) {
                transform.apply(doc, cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ElementExt, HtmlElement, HtmlNode, HtmlTag, SrcSet, Transform, Transforms, tag};
    use typst::syntax::Span;

    fn candidates(srcset: &str) -> Vec<(&str, &str)> {
        SrcSet(srcset).candidates()
    }

    fn element(name: HtmlTag, children: Vec<HtmlNode>) -> HtmlElement {
        let mut el = HtmlElement::new(name);
        for child in children {
            el.children.push(child);
        }
        el
    }

    fn text(value: &str) -> HtmlNode {
        HtmlNode::Text(value.into(), Span::detached())
    }

    /// `== The *fast* way to a #emph[slug]`, as typst emits it: text, an
    /// element, text, an element.
    #[test]
    fn text_reads_mixed_content_in_document_order() {
        let heading = element(
            tag::h2,
            vec![
                text("The "),
                element(tag::em, vec![text("fast")]).into(),
                text(" way to a "),
                element(tag::span, vec![text("slug")]).into(),
            ],
        );

        assert_eq!(heading.text(), "The fast way to a slug");
    }

    /// `= #svg("/star.svg") The fast way`, once the icon has been inlined: its
    /// `<title>` is an accessible name for the icon, not words in the heading.
    #[test]
    fn text_does_not_read_what_a_reader_does_not() {
        let svg = HtmlTag::intern("svg").expect("svg");
        let title = HtmlTag::intern("title").expect("title");
        let mut hidden = element(tag::span, vec![text("#")]);
        hidden.attrs.push(super::attr::aria_hidden, "true");
        let heading = element(
            tag::h1,
            vec![
                element(svg, vec![element(title, vec![text("A gold star")]).into()]).into(),
                text("The fast way"),
                hidden.into(),
            ],
        );

        assert_eq!(heading.text(), "The fast way");
    }

    /// Nesting is followed at the point it occurs, however deep.
    #[test]
    fn text_descends_where_the_nesting_is() {
        let heading = element(
            tag::h3,
            vec![
                element(
                    tag::a,
                    vec![text("a "), element(tag::code, vec![text("raw")]).into()],
                )
                .into(),
                text(" tail"),
            ],
        );

        assert_eq!(heading.text(), "a raw tail");
    }

    /// The flat case the inline `<script>`/`<style>` digests rely on.
    #[test]
    fn text_concatenates_flat_children_as_written() {
        let script = element(tag::script, vec![text("let a = 1;"), text("let b = 2;")]);

        assert_eq!(script.text(), "let a = 1;let b = 2;");
    }

    #[test]
    fn srcset_candidates_split_on_descriptors_and_bare_commas() {
        assert_eq!(
            candidates("/a.png 1x, /b.png 2x"),
            vec![("/a.png", "1x"), ("/b.png", "2x")]
        );
        assert_eq!(
            candidates("/a.png, /b.png 2x"),
            vec![("/a.png", ""), ("/b.png", "2x")]
        );
    }

    #[test]
    fn srcset_candidates_keep_data_uri_commas_intact() {
        assert_eq!(
            candidates("/a.png 1x, data:image/png;base64,AAA 2x"),
            vec![("/a.png", "1x"), ("data:image/png;base64,AAA", "2x")]
        );
    }

    /// Every `after` on a pass of `passes`, as `(pass, dependency)`.
    fn edges(passes: &[Box<dyn Transform>]) -> Vec<(&'static str, &'static str)> {
        passes
            .iter()
            .flat_map(|pass| pass.after().iter().map(|dep| (pass.name(), *dep)))
            .collect()
    }

    fn at(order: &[&'static str], name: &str) -> usize {
        order
            .iter()
            .position(|written| *written == name)
            .unwrap_or_else(|| panic!("`{name}` is not a pass in the pipeline"))
    }

    #[test]
    fn every_pass_is_named_once() {
        let mut names = Transforms::builtin().names();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "two passes share a name");
    }

    /// Every dependency names a pass that is actually in the pipeline. Left to
    /// [`Transforms::ordered`] this is a panic about a cycle; here it names the
    /// pass that cannot be found.
    #[test]
    fn every_dependency_names_a_pass() {
        let passes = Transforms::declared();
        let names: Vec<&'static str> = passes.iter().map(|pass| pass.name()).collect();
        for (pass, dep) in edges(&passes) {
            assert!(
                names.contains(&dep),
                "`{pass}` runs after `{dep}`, which does not exist"
            );
        }
    }

    /// The hand-written list is already a valid linearization, so sorting it
    /// changes nothing. A reorder that breaks a declared constraint is fixed by
    /// [`Transforms::ordered`] and reported here.
    #[test]
    fn the_written_order_already_satisfies_every_constraint() {
        let written: Vec<&'static str> = Transforms::declared()
            .iter()
            .map(|pass| pass.name())
            .collect();
        assert_eq!(
            Transforms::builtin().names(),
            written,
            "the pipeline as written is not the order its constraints put it in"
        );
    }

    /// The sort is what holds the pipeline up, not the order the passes happen
    /// to be written in: reversed, they still come back with every dependency
    /// ahead of the pass that named it.
    #[test]
    fn a_scrambled_pipeline_sorts_back_into_a_valid_one() {
        let mut scrambled = Transforms::declared();
        scrambled.reverse();
        let sorted = Transforms::ordered(scrambled);
        let order: Vec<&'static str> = sorted.iter().map(|pass| pass.name()).collect();
        for (pass, dep) in edges(&sorted) {
            assert!(
                at(&order, dep) < at(&order, pass),
                "`{dep}` must run before `{pass}`, and does not"
            );
        }
    }
}
