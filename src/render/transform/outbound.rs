//! Collects the outbound links a page carries, for `check --external`.
//!
//! Only anchors: an outbound link is something a *reader* can follow and find
//! gone. A stylesheet or script from a CDN fails loudly in the browser and is
//! not the site's link rot to report.
//!
//! Collected here rather than by scanning the rendered text, because at this
//! point the DOM already says which values are `href`s, and no `http` inside a
//! code block or a paragraph can be mistaken for one.

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;

use super::{Cx, ElementExt, Transform};

/// The [`Transform`] that records outbound `http(s)` anchors.
pub(super) struct Outbound;

impl Transform for Outbound {
    fn enabled(&self, config: &Config) -> bool {
        config.links.external
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        doc.root_mut().walk(&mut |el| {
            if el.tag != tag::a {
                return;
            }
            let Some(href) = el.attrs.get(attr::href) else {
                return;
            };
            // Deduplicated per page: a nav repeated on every page is one URL to
            // check, and one page to name when it is dead.
            if Outbound::is_external(href) && !cx.found.external.iter().any(|seen| seen == href) {
                cx.found.external.push(href.to_string());
            }
        });
    }
}

impl Outbound {
    /// Whether a href names something out on the web that can be requested.
    ///
    /// Scheme-relative (`//host/x`) is excluded on purpose: it resolves against
    /// the page's own scheme, which a static build does not know.
    fn is_external(href: &str) -> bool {
        href.starts_with("http://") || href.starts_with("https://")
    }
}

#[cfg(test)]
mod tests {
    use super::Outbound;

    #[test]
    fn only_absolute_web_urls_are_external() {
        assert!(Outbound::is_external("https://example.com/a"));
        assert!(Outbound::is_external("http://example.com"));
        for internal in [
            "/posts/a/",
            "posts/a.typ",
            "#section",
            "mailto:x@example.com",
            "tel:+123",
            "//cdn.example.com/x.js",
        ] {
            assert!(!Outbound::is_external(internal), "{internal}");
        }
    }
}
