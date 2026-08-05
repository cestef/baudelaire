//! Per-page transforms over the typed HTML DOM.
//!
//! A [`Transform`] rewrites a page's [`HtmlDocument`] in place before
//! serialization: the render-side counterpart to a post-build
//! [`crate::engine`] `Processor`. [`Transforms::builtin`] is the single source
//! of the DOM pipeline: a new pass is one `impl Transform` plus one line in that
//! list, each gated on its own config. Even core link resolution is a transform.

mod anchors;
mod base;
mod embed;
mod externalize;
mod fingerprint;
mod footnotes;
mod highlight;
mod image;
mod integrity;
mod lang;
mod meta;
mod outbound;
mod rewrite;
mod sources;
mod spans;
mod speculation;
#[cfg(feature = "announce")]
mod standard;
mod svg;

pub use externalize::ImageRef;

use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, HtmlTag, attr, tag};

use crate::config::Config;
use crate::content::Page;

use super::AssetMap;
use super::LinkMap;
use super::SrcSets;
use anchors::Anchors;
use base::BasePath;
use embed::Embed;
use externalize::Externalize;
use fingerprint::Fingerprint;
use footnotes::Footnotes;
use highlight::Highlight;
use image::Images;
use integrity::Integrity;
use lang::Lang;
use meta::Meta;
use outbound::Outbound;
use rewrite::Links;
use sources::Sources;
use spans::Spans;
use speculation::Speculation;
#[cfg(feature = "announce")]
use standard::Verify;
use svg::Svg;

/// Per-page context handed to every transform. Transforms run sequentially for
/// a page, so they share this one mutable accumulator.
pub(super) struct Cx<'a> {
    pub config: &'a Config,
    pub page: &'a Page,
    pub links: &'a LinkMap,
    /// Processed-asset URL map, consumed by the fingerprint and meta transforms.
    pub assets: &'a AssetMap,
    /// Responsive width-variant manifest, consumed by the sources transform.
    pub srcsets: &'a SrcSets,
    /// What this build wrote and what each file digests to, consumed by the
    /// integrity transform.
    pub emitted: &'a super::Emitted,
    /// Project root, so the externalize and svg transforms resolve a marker's
    /// project-relative path to the source file on disk.
    pub root: &'a std::path::Path,
    /// The content tree as the compiler spells it, resolved once for the build:
    /// what a link's origin is tested against to tell an author's own reference
    /// from a layout's chrome.
    pub content: &'a std::path::Path,
    /// The world this page compiled in, so a transform can ask what a node's
    /// span points at: the source files, as the compiler read them.
    pub world: &'a crate::world::PageWorld,
    /// What the pipeline has found so far. This is the value the caller gets
    /// back, accumulated in place rather than copied out field by field.
    pub found: super::Rewrite,
    /// The width variants this page's *extracted* images will be given, keyed
    /// by the URL each is served at.
    ///
    /// Page-local scratch rather than part of [`SrcSets`]: the pipeline's
    /// manifest is built before any page renders, and an image lifted out of a
    /// page is not known to exist until that page has rendered. Written by
    /// [`Externalize`] and read by [`Sources`], which run in that order, so the
    /// two kinds of responsive image reach the `srcset` writer the same way.
    pub extracted: std::collections::BTreeMap<String, Vec<super::Candidate>>,
}

/// The attributes that carry a URL to an asset this site owns.
///
/// One list, because four hand-written copies had drifted into four different
/// sets: `og:image` was fingerprinted but never base-path-prefixed and never
/// embedded, so a subpath-hosted site's social card pointed at a file that was
/// not there. `srcset` is handled separately by [`ElementExt::assets`], which
/// parses its candidate list, and `content` by [`URL_META`], which is not
/// unconditional.
const URL_ATTRS: &[HtmlAttr] = &[attr::href, attr::src, attr::poster];

/// OpenGraph names its tags with `property` where the HTML spec uses `name`.
/// typst-html has no constant for it, since it is RDFa rather than HTML. Here
/// rather than on the pass that writes the tags, because the pass that decides
/// whether their `content` is a URL reads the same attribute.
pub(super) const PROPERTY: HtmlAttr = HtmlAttr::constant("property");

/// The `<meta>` keys whose `content` is a URL.
///
/// Every other one is prose: a title, a description, an author, a tag. `content`
/// was in [`URL_ATTRS`] outright for a while, which made it a URL on every
/// `<meta>` in the page. The asset passes were saved by their own prefix check
/// (nothing that is not under `/assets/` can match), but the base path has none
/// to make: it prefixes any value starting with `/`, so a site at `/docs` and a
/// page titled `/etc/hosts, annotated` published
/// `og:title` = `/docs/etc/hosts, annotated`.
const URL_META: &[&str] = &[
    "og:image",
    "og:image:url",
    "og:image:secure_url",
    "og:url",
    "twitter:image",
];

/// The elements a reader deep-links to and an outline is read from, in level
/// order, so an index into this *is* the heading level. One list, because three
/// passes ask about headings: the anchor pass, the lint that reports a skipped
/// level, and anything that comes after them.
const HEADINGS: &[HtmlTag] = &[tag::h1, tag::h2, tag::h3, tag::h4, tag::h5, tag::h6];

/// The one replace-or-push rule for an attribute list, shared by
/// [`ElementExt::set`] and by any pass still assembling attributes that has no
/// element to hang them off yet.
pub(super) trait AttrsExt {
    /// Set `key` to `value`, replacing an existing entry rather than appending
    /// a duplicate (which is invalid HTML).
    fn set(&mut self, key: HtmlAttr, value: &str);
    /// Drop `key` if present.
    ///
    /// The counterpart to [`set`](AttrsExt::set), for a transform that moves a
    /// value *out* of an attribute rather than changing it: emptying a `style`
    /// leaves `style=""` in the markup, which is not the same as not having one.
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
    /// Visit this element, then every descendant element, depth-first. The one
    /// shared walk over the typed DOM, so no transform hand-rolls its own
    /// recursion.
    fn walk(&mut self, f: &mut impl FnMut(&mut HtmlElement));
    /// The same walk, read-only: what a pass that only *looks* at the DOM
    /// takes, since [`walk`](ElementExt::walk) reaches for the mutable children
    /// of every element it passes and would copy each shared list to hand one
    /// out.
    fn visit(&self, f: &mut impl FnMut(&HtmlElement));
    /// The text this element and its descendants carry, markup dropped, **in
    /// document order**: what a heading's anchor is slugged from, and the bytes
    /// an inline `<script>` or `<style>` ships.
    ///
    /// The order is the whole contract. Built on [`visit`](ElementExt::visit)
    /// plus a scan of each element's own `Text` children, it was not document
    /// order for mixed content: an element's direct text came out before
    /// anything nested inside it, so `== The *fast* way` slugged to
    /// `the-way-fast` and every heading carrying emphasis, a link, `raw`, math
    /// or a footnote marker got a scrambled `id`. Ids are the site's deep-link
    /// surface and the fragment check reads the same set, so a *correct*
    /// `#link("page.typ#the-fast-way")` failed the build.
    fn text(&self) -> String;
    /// This element's heading level, `1`..`6`, or `None` for anything that is
    /// not a heading.
    fn heading(&self) -> Option<u8>;
    /// Whether this element pulls in a stylesheet. `rel` may hold several
    /// tokens (`rel="preload stylesheet"`), which is why it is not a string
    /// comparison, and why it is written once.
    fn stylesheet(&self) -> bool;
    /// This element's `<head>` child, if it has one: the one place a transform
    /// appends head elements, so meta and verification tags find it the same way.
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
        // One in-order walk over `children`, pushing each `Text` where it
        // occurs and descending into an `Element` at that same point. An
        // explicit stack rather than recursion, so a deeply nested document
        // cannot overflow the one the compiler gave us; reversed on push, so
        // popping yields the children back in the order they were written.
        let mut stack: Vec<&HtmlNode> = self.children.iter().rev().collect();
        while let Some(node) = stack.pop() {
            match node {
                HtmlNode::Text(text, _) => out.push_str(text),
                HtmlNode::Element(child) => stack.extend(child.children.iter().rev()),
                _ => {}
            }
        }
        out
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

/// Document-level entry points, so a transform states what it visits rather
/// than repeating `root_mut()` and its own closure plumbing.
pub(super) trait DocumentExt {
    /// Visit every element in the document, depth-first.
    fn walk(&mut self, f: impl FnMut(&mut HtmlElement));
    /// The same, read-only: what the lint pass takes.
    fn visit(&self, f: impl FnMut(&HtmlElement));
    /// The document's `<head>`, if the page has one.
    fn head(&mut self) -> Option<&mut HtmlElement>;
    /// Rewrite every asset-bearing URL in the document through `f`.
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
                let url = match f(url) {
                    Some(new) => {
                        changed = true;
                        new
                    }
                    None => url.to_owned(),
                };
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
                // the comma belonged to the separator, not the URL: no descriptor.
                out.push((trimmed, ""));
                rest = tail;
                continue;
            }
            let (descriptor, after) = match tail.find(',') {
                Some(i) => (&tail[..i], &tail[i + 1..]),
                None => (tail, ""),
            };
            out.push((url, descriptor.trim()));
            rest = after;
        }
        out
    }
}

/// A per-page pass over the typed HTML DOM. `Send + Sync` because the owning
/// [`super::Renderer`] is shared read-only across the parallel compile pool.
pub(super) trait Transform: Send + Sync {
    /// Whether to run, from config alone. Keeps the gate declarative.
    fn enabled(&self, config: &Config) -> bool;
    /// Rewrite `doc` in place, optionally recording findings in `cx`.
    /// Best-effort: a transform that cannot act on a node leaves it untouched.
    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>);
}

/// The built-in transforms, in apply order (link resolution first).
pub(super) struct Transforms(Vec<Box<dyn Transform>>);

impl Transforms {
    pub(super) fn builtin() -> Self {
        // order matters: resolve links, annotate, add responsive srcsets, inline
        // embeds (as `data:` URIs), fingerprint whatever refs remain (the srcset
        // URLs among them), then shift them under the base path.
        Self(vec![
            Box::new(Links),
            // Early, so every later pass sees an inlined icon as ordinary DOM:
            // an `<image href>` inside one is fingerprinted and base-pathed
            // like any other reference.
            Box::new(Svg),
            Box::new(Lang),
            Box::new(Anchors),
            // Before anything that reads the page's structure: every later pass
            // should see the notes where they will be served, not where typst
            // left them.
            Box::new(Footnotes),
            Box::new(Highlight),
            // After the passes that move authored elements around, before the
            // ones that synthesize elements of our own: an element is stamped
            // where it ends up, and only what the author actually wrote carries
            // a location at all.
            Box::new(Spans),
            Box::new(Meta),
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
            // Last, over the finished markup: an `integrity` names the file a
            // browser will actually fetch, base path and content hash and all,
            // and an inline digest has to cover the bytes as they are served.
            Box::new(Integrity),
        ])
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
    use super::{ElementExt, HtmlElement, HtmlNode, HtmlTag, SrcSet, tag};
    use typst::syntax::Span;

    fn candidates(srcset: &str) -> Vec<(&str, &str)> {
        SrcSet(srcset).candidates()
    }

    /// An element carrying `children`, in order: what mixed content looks like
    /// once typst has emitted it.
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
    /// element, text, an element. The anchor is slugged from this, so reading
    /// an element's own text before its children's spelled the heading back in
    /// an order nobody wrote and gave the section an id no correct deep link
    /// could ever name.
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

    /// Nesting is followed at the point it occurs, however deep, and an element
    /// with no text of its own contributes nothing rather than reordering what
    /// surrounds it.
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

    /// The flat case the inline `<script>`/`<style>` digests rely on: one
    /// element, text children only, concatenated as written.
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
}
