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
mod image;
mod lang;
mod meta;
mod outbound;
mod rewrite;
mod sources;
mod speculation;
mod standard;
mod svg;

pub use externalize::ImageRef;

use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, attr, tag};

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
use image::Images;
use lang::Lang;
use meta::Meta;
use outbound::Outbound;
use rewrite::Links;
use sources::Sources;
use speculation::Speculation;
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
    /// Project root, so the externalize transform resolves image markers to
    /// source files.
    pub root: &'a std::path::Path,
    /// What the pipeline has found so far. This is the value the caller gets
    /// back, accumulated in place rather than copied out field by field.
    pub found: super::Rewrite,
}

/// The one shared walker over the typed DOM, so every transform visits
/// elements the same way instead of hand-rolling its own recursion.
/// The attributes that carry a URL to an asset this site owns.
///
/// One list, because four hand-written copies had drifted into four different
/// sets: `og:image` (a `content` attribute) was fingerprinted but never
/// base-path-prefixed and never embedded, so a subpath-hosted site's social card
/// pointed at a file that was not there. `srcset` is handled separately by
/// [`ElementExt::assets`], which parses its candidate list.
pub(super) const URL_ATTRS: &[HtmlAttr] = &[attr::href, attr::src, attr::poster, attr::content];

/// The one replace-or-push rule for an attribute list, shared by
/// [`ElementExt::set`] and by any pass still assembling attributes that has no
/// element to hang them off yet.
pub(super) trait AttrsExt {
    /// Set `key` to `value`, replacing an existing entry rather than appending
    /// a duplicate (which is invalid HTML).
    fn set(&mut self, key: HtmlAttr, value: &str);
}

impl AttrsExt for typst_html::HtmlAttrs {
    fn set(&mut self, key: HtmlAttr, value: &str) {
        match self.get_mut(key) {
            Some(existing) => *existing = value.into(),
            None => self.push(key, value),
        }
    }
}

pub(super) trait ElementExt {
    /// Visit this element, then every descendant element, depth-first.
    fn walk(&mut self, f: &mut impl FnMut(&mut HtmlElement));
    /// This element's `<head>` child, if it has one: the one place a transform
    /// appends head elements, so meta and verification tags find it the same way.
    fn head(&mut self) -> Option<&mut HtmlElement>;
    /// Set `key` to `value`, replacing an existing attribute or appending one.
    fn set(&mut self, key: HtmlAttr, value: &str);
    /// Rewrite each attribute among `keys` that is present: `f` returns the
    /// replacement value, or `None` to leave it as authored.
    fn rewrite(&mut self, keys: &[HtmlAttr], f: impl FnMut(&str) -> Option<String>);
    /// Rewrite every asset-bearing attribute (the `keys` plus `srcset`) through
    /// `f` in one pass, so an asset-rewriting transform states only its key list.
    fn assets(&mut self, keys: &[HtmlAttr], f: impl FnMut(&str) -> Option<String>);
    /// Rewrite each URL in a `srcset` attribute (a comma-separated list of
    /// `url [descriptor]` candidates) leaving descriptors intact. `f` maps one
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

    fn assets(&mut self, keys: &[HtmlAttr], mut f: impl FnMut(&str) -> Option<String>) {
        self.rewrite(keys, &mut f);
        self.rewrite_srcset(&mut f);
    }

    fn rewrite_srcset(&mut self, mut f: impl FnMut(&str) -> Option<String>) {
        let Some(value) = self.attrs.get_mut(attr::srcset) else {
            return;
        };
        let mut changed = false;
        let rebuilt = candidates(value)
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
        if changed {
            *value = rebuilt.into();
        }
    }
}

/// Split a `srcset` into `(url, descriptor)` candidates by the HTML spec's
/// whitespace-driven rule: a URL runs to the next whitespace and may itself
/// contain commas (`data:` URIs), so only a URL's *trailing* commas (or a
/// comma after the descriptor) terminate a candidate.
fn candidates(srcset: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = srcset;
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
            Box::new(Meta),
            Box::new(Speculation),
            Box::new(Outbound),
            Box::new(Verify),
            Box::new(Images),
            Box::new(Externalize),
            Box::new(Sources),
            Box::new(Embed),
            Box::new(Fingerprint),
            Box::new(BasePath),
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
    use super::candidates;

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
