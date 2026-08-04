//! Turns each `<img>` that names a responsive source into one carrying a
//! `srcset`.
//!
//! When `assets { images { responsive { .. } } }` is set, the asset pipeline generates
//! downscaled copies and records them in a [`super::SrcSets`] manifest. This
//! transform looks up each image's `src`, and when the manifest has variants for
//! it, fills a `srcset` (the variant URLs with their width descriptors). An
//! image lifted out of the page itself has no manifest entry, since it does not
//! exist until the page renders; [`super::Externalize`] leaves its candidates on
//! the page context instead, and they are written the same way. It adds
//! a `sizes` only when the config supplies one: a `w`-descriptor `srcset` with
//! no `sizes` already means `100vw` to the browser, so emitting that default
//! would waste bytes; a theme that knows its content width sets `sizes` to stop
//! wide viewports over-fetching. Both are best-effort: an image the author
//! already gave a `srcset` is skipped, and the `src` fallback stays, so a
//! browser without `srcset` support still loads the original.

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;

use super::{Cx, DocumentExt, Transform};
use crate::render::Tail;

/// The [`Transform`] that annotates responsive images with a `srcset`.
pub(super) struct Sources;

impl Transform for Sources {
    fn enabled(&self, config: &Config) -> bool {
        config.assets.images.responsive.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        doc.walk(|element| {
            if element.tag != tag::img {
                return;
            }
            // never override an author-provided srcset.
            if element.attrs.get(attr::srcset).is_some() {
                return;
            }
            let Some(src) = element.attrs.get(attr::src) else {
                return;
            };
            // match on the path alone, ignoring any query/fragment.
            let path = Tail::of(src).path;
            // An image this page had lifted out of itself carries the widths
            // the copy pass is about to cut; anything else is looked up in the
            // pipeline's manifest. Both are `(url, width)` candidates by the
            // time they get here, so the attribute is written once.
            let extracted = cx.extracted.get(path);
            let variants = cx.srcsets.candidates(path);
            // The manifest probe is recorded whether or not it matched: an
            // image with no variants today gets some when the responsive widths
            // change, and this page has to pick up the new srcset. An extracted
            // image needs no probe: its source is a file typst read, so an edit
            // to it recompiles this page anyway.
            if extracted.is_none() {
                cx.found.srcsets.extend(variants.probed);
            }
            let Some(candidates) = extracted.map(Vec::as_slice).or(variants.candidates) else {
                return;
            };
            // `url 480w` candidates, in ascending width order.
            let srcset = candidates
                .iter()
                .map(|c| format!("{} {}w", c.url, c.width))
                .collect::<Vec<_>>()
                .join(", ");
            element.attrs.push(attr::srcset, srcset);
            // only emit sizes the config provides; an absent sizes is 100vw by
            // spec, and an author-set one is left as is.
            if let Some(sizes) = &cx.config.assets.images.responsive.sizes
                && element.attrs.get(attr::sizes).is_none()
            {
                element.attrs.push(attr::sizes, sizes.clone());
            }
        });
    }
}
