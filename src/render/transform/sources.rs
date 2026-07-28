//! Turns each `<img>` that names a responsive source into one carrying a
//! `srcset`.
//!
//! When `images { responsive { .. } }` is set, the asset pipeline generates
//! downscaled copies and records them in a [`super::SrcSets`] manifest. This
//! transform looks up each image's `src`, and when the manifest has variants for
//! it, fills a `srcset` (the variant URLs with their width descriptors). It adds
//! a `sizes` only when the config supplies one: a `w`-descriptor `srcset` with
//! no `sizes` already means `100vw` to the browser, so emitting that default
//! would waste bytes; a theme that knows its content width sets `sizes` to stop
//! wide viewports over-fetching. Both are best-effort: an image the author
//! already gave a `srcset` is skipped, and the `src` fallback stays, so a
//! browser without `srcset` support still loads the original.

use std::fmt::Write;

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;

use super::{Cx, ElementExt, Transform};
use crate::render::Tail;

/// The [`Transform`] that annotates responsive images with a `srcset`.
pub(super) struct Sources;

impl Transform for Sources {
    fn enabled(&self, config: &Config) -> bool {
        config.images.responsive.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        doc.root_mut().walk(&mut |element| {
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
            // match the manifest on the path alone, ignoring any query/fragment.
            let Some(candidates) = cx.srcsets.candidates(Tail::of(src).path) else {
                return;
            };
            let srcset = candidates.iter().fold(String::new(), |mut acc, c| {
                if !acc.is_empty() {
                    acc.push_str(", ");
                }
                let _ = write!(acc, "{} {}w", c.url, c.width);
                acc
            });
            element.attrs.push(attr::srcset, srcset);
            // only emit sizes the config provides; an absent sizes is 100vw by
            // spec, and an author-set one is left as is.
            if let Some(sizes) = &cx.config.images.responsive.sizes
                && element.attrs.get(attr::sizes).is_none()
            {
                element.attrs.push(attr::sizes, sizes.clone());
            }
        });
    }
}
