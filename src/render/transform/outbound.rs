//! Collects the outbound anchors a page carries, for `check --external`.

use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;

use super::{Cx, DocumentExt, Transform};

/// The [`Transform`] that records outbound `http(s)` anchors.
pub(super) struct Outbound;

impl Transform for Outbound {
    fn enabled(&self, config: &Config) -> bool {
        config.links.external.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        doc.walk(|el| {
            if el.tag != tag::a {
                return;
            }
            let Some(href) = el.attrs.get(attr::href) else {
                return;
            };
            if Self::is_external(href) && !cx.found.external.iter().any(|seen| seen == href) {
                cx.found.external.push(href.to_string());
            }
        });
    }
}

impl Outbound {
    /// Whether a href names something out on the web that can be requested.
    ///
    /// Scheme-relative (`//host/x`) is excluded: it resolves against the page's
    /// own scheme, which a static build does not know.
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
