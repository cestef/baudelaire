//! Improves `<img>` loading behaviour: `loading="lazy"` and `decoding="async"`
//! on every image that does not already carry them.

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;

use super::{Cx, DocumentExt, Exempt, Transform};

/// The [`Transform`] that annotates images for lazy, async loading.
pub(super) struct Images;

impl Images {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "images";
}

impl Transform for Images {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

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
