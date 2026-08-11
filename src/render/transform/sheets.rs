//! Links the assets the build owns from the pages that get them.
//!
//! One pass for every owned asset, rather than a `<link>` written by hand
//! wherever an asset happened to be added: the registry says which assets exist
//! ([`crate::owned`]), which of them this site serves, and whether every page
//! gets one or only the pages that asked. What is left here is the part that
//! was identical each time, and the part a second asset got wrong first: the
//! link is spelled the way an author would spell it, so the fingerprint, embed
//! and base-path passes reach it without knowing it was synthesized, and the
//! name is recorded so the pipeline writes the file it reserved.

use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::Config;
use crate::owned::{Owned, builtin};

use super::{Cx, DocumentExt, Transform};

/// The [`Transform`] that puts a `<link>` to each owned stylesheet in a page's
/// `<head>`.
pub(super) struct Sheets;

impl Transform for Sheets {
    /// Whenever this site serves any of them. A site that serves none never
    /// reaches the walk.
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
        // Best-effort, like every other head pass: a page that emitted its own
        // root has no `<head>`, so there is nowhere to put a link.
        let Some(head) = doc.head() else {
            return;
        };
        for rel in wanted {
            head.children.push(Self::link(&rel, cx.config));
            // What tells the pipeline to write the file it named: it reserved
            // one before any page existed, and a page carrying the link is what
            // makes it wanted.
            cx.found.owned.insert(rel);
        }
    }
}

impl Sheets {
    /// Whether this page gets `asset`: every page, unless the asset is one a
    /// page has to ask for, in which case the pass that decides has already
    /// recorded the name.
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
