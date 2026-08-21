//! Links the assets the build owns ([`crate::owned`]) from the pages that get
//! them.

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::Config;
use crate::owned::{Owned, builtin};

use super::{Cx, DocumentExt, Exempt, Transform};

/// The [`Transform`] that puts a `<link>` to each owned stylesheet in a page's
/// `<head>`. Recording the name in `found.owned` is what makes the pipeline
/// write the file it reserved.
pub(super) struct Sheets;

impl Transform for Sheets {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

    fn enabled(&self, config: &Config) -> bool {
        builtin().iter().any(|asset| asset.serves(config))
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let wanted: Vec<String> = builtin()
            .iter()
            .filter(|asset| asset.serves(cx.config) && Self::wanted(asset.as_ref(), cx))
            .map(|asset| asset.rel(cx.config).to_string_lossy().into_owned())
            .collect();
        if wanted.is_empty() {
            return;
        }
        let Some(head) = doc.head() else {
            return;
        };
        for rel in wanted {
            head.children.push(Self::link(&rel, cx.config));
            cx.found.owned.insert(rel);
        }
    }
}

impl Sheets {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "sheets";

    /// Whether this page gets `asset`: every page, unless it is one a page has
    /// to ask for, in which case an earlier pass recorded its name.
    fn wanted(asset: &dyn Owned, cx: &Cx<'_>) -> bool {
        asset.everywhere()
            || cx
                .found
                .owned
                .contains(asset.rel(cx.config).to_string_lossy().as_ref())
    }

    /// The `<link>` to a served stylesheet, spelled as an authored reference
    /// would be.
    fn link(rel: &str, config: &Config) -> HtmlNode {
        HtmlElement::new(tag::link)
            .with_attr(attr::rel, "stylesheet")
            .with_attr(attr::href, config.asset_url(std::path::Path::new(rel)))
            .into()
    }
}
