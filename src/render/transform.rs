//! Per-page transforms over the typed HTML DOM.
//!
//! A [`Transform`] rewrites a page's [`HtmlDocument`] in place before
//! serialization — the render-side counterpart to a post-build
//! [`crate::engine`] `Processor`. [`Transforms::builtin`] is the single source
//! of the DOM pipeline: a new pass is one `impl Transform` plus one line in that
//! list, each gated on its own config. Even core link resolution is a transform.

use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::Config;
use crate::content::Page;

use super::AssetMap;
use super::LinkMap;
use super::anchors::Anchors;
use super::embed::Embed;
use super::fingerprint::Fingerprint;
use super::image::Images;
use super::meta::Meta;
use super::rewrite::Links;
use super::standard::Verify;

/// Per-page context handed to every transform. Transforms run sequentially for
/// a page, so they share this one mutable accumulator.
pub(super) struct Cx<'a> {
    pub config: &'a Config,
    pub page: &'a Page,
    pub links: &'a LinkMap,
    /// Processed-asset URL map, consumed by the fingerprint and meta transforms.
    pub assets: &'a AssetMap,
    /// Raw targets of internal `.typ` links with no matching page, collected for
    /// link checking.
    pub broken: Vec<String>,
}

/// The one shared walker over the typed DOM, so every transform visits
/// elements the same way instead of hand-rolling its own recursion.
pub(super) trait ElementExt {
    /// Visit this element, then every descendant element, depth-first.
    fn walk(&mut self, f: &mut impl FnMut(&mut HtmlElement));
    /// This element's `<head>` child, if it has one — the one place a transform
    /// appends head elements, so meta and verification tags find it the same way.
    fn head(&mut self) -> Option<&mut HtmlElement>;
    /// Rewrite each attribute among `keys` that is present: `f` returns the
    /// replacement value, or `None` to leave it as authored.
    fn rewrite(&mut self, keys: &[HtmlAttr], f: impl FnMut(&str) -> Option<String>);
    /// Rewrite every asset-bearing attribute — the `keys` plus `srcset` — through
    /// `f` in one pass, so an asset-rewriting transform states only its key list.
    fn assets(&mut self, keys: &[HtmlAttr], f: impl FnMut(&str) -> Option<String>);
    /// Rewrite each URL in a `srcset` attribute — a comma-separated list of
    /// `url [descriptor]` candidates — leaving descriptors intact. `f` maps one
    /// URL to its replacement, or `None` to keep it. So `<img srcset>` and
    /// `<source srcset>` get the same asset rewriting as plain `src`.
    fn rewrite_srcset(&mut self, f: impl FnMut(&str) -> Option<String>);
}

impl ElementExt for HtmlElement {
    fn walk(&mut self, f: &mut impl FnMut(&mut HtmlElement)) {
        f(self);
        for child in self.children.make_mut() {
            if let HtmlNode::Element(child) = child {
                child.walk(f);
            }
        }
    }

    fn head(&mut self) -> Option<&mut HtmlElement> {
        self.children.make_mut().iter_mut().find_map(|node| match node {
            HtmlNode::Element(el) if el.tag == tag::head => Some(el),
            _ => None,
        })
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

    fn assets(&mut self, keys: &[HtmlAttr], mut f: impl FnMut(&str) -> Option<String>) {
        self.rewrite(keys, &mut f);
        self.rewrite_srcset(&mut f);
    }

    fn rewrite_srcset(&mut self, mut f: impl FnMut(&str) -> Option<String>) {
        let Some(value) = self.attrs.get_mut(attr::srcset) else {
            return;
        };
        let mut changed = false;
        // Comma-separates candidates; splitting on `,` is exact for the asset
        // URLs we emit (fingerprinted filenames never contain commas).
        let rebuilt = value
            .split(',')
            .map(|candidate| {
                let candidate = candidate.trim();
                let (url, descriptor) = candidate
                    .split_once(char::is_whitespace)
                    .map_or((candidate, ""), |(u, d)| (u, d.trim_start()));
                match f(url) {
                    Some(new) if descriptor.is_empty() => {
                        changed = true;
                        new
                    }
                    Some(new) => {
                        changed = true;
                        format!("{new} {descriptor}")
                    }
                    None => candidate.to_owned(),
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        if changed {
            *value = rebuilt.into();
        }
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
        // Order matters: resolve links, add meta/image annotations, then inline
        // embeds (produces `data:` URIs), then fingerprint whatever asset
        // references remain.
        Self(vec![
            Box::new(Links),
            Box::new(Anchors),
            Box::new(Meta),
            Box::new(Verify),
            Box::new(Images),
            Box::new(Embed),
            Box::new(Fingerprint),
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
