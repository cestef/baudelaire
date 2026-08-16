//! Turns each `<img>` that names a responsive source into one carrying a
//! `srcset`, from the pipeline's manifest or from what the page lifted out of
//! itself.

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;

use super::{Cx, DocumentExt, Transform};
use crate::render::Tail;

/// The [`Transform`] that annotates responsive images with a `srcset`.
///
/// A manifest probe is recorded whether or not it matched: an image with no
/// variants today gets some when the responsive widths change, and this page
/// has to pick up the new `srcset`.
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
            if element.attrs.get(attr::srcset).is_some() {
                return;
            }
            let Some(src) = element.attrs.get(attr::src) else {
                return;
            };
            let path = Tail::of(src).path;
            let extracted = cx.extracted.get(path);
            let variants = cx.srcsets.candidates(path);
            if extracted.is_none() {
                cx.found.srcsets.extend(variants.probed);
            }
            let Some(candidates) = extracted.map(Vec::as_slice).or(variants.candidates) else {
                return;
            };
            let srcset = candidates
                .iter()
                .map(|c| format!("{} {}w", c.url, c.width))
                .collect::<Vec<_>>()
                .join(", ");
            element.attrs.push(attr::srcset, srcset);
            if let Some(sizes) = &cx.config.assets.images.responsive.sizes
                && element.attrs.get(attr::sizes).is_none()
            {
                element.attrs.push(attr::sizes, sizes.clone());
            }
        });
    }
}
