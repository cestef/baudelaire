//! Improves `<img>` loading behaviour: `loading="lazy"` and `decoding="async"`
//! on every image that does not already carry them.

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;

use super::{Cx, DocumentExt, Transform};

/// The [`Transform`] that annotates images for lazy, async loading.
pub(super) struct Images;

impl Transform for Images {
    fn enabled(&self, config: &Config) -> bool {
        config.assets.images.lazy
    }

    fn apply(&self, doc: &mut HtmlDocument, _cx: &mut Cx<'_>) {
        doc.walk(|element| {
            if element.tag == tag::img {
                if element.attrs.get(attr::loading).is_none() {
                    element.attrs.push(attr::loading, "lazy");
                }
                if element.attrs.get(attr::decoding).is_none() {
                    element.attrs.push(attr::decoding, "async");
                }
            }
        });
    }
}
