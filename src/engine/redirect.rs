//! Redirect stubs: a minimal HTML page that forwards a stale URL to its new one.

use super::process::{Emit, Processor, Site};
use super::xml::Xml;
use crate::error::Result;
use crate::ui::Count;

/// Emits a redirect stub for every `redirect` old-path in a page's
/// frontmatter, forwarding it to that page's permalink.
pub(super) struct Redirects;

impl Processor for Redirects {
    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let mut count = 0usize;
        for page in site.pages {
            for old in &page.frontmatter.redirect {
                out.file(&site.config.destination(old), &Self::stub(&page.permalink))?;
                count += 1;
            }
        }
        if count > 0 {
            out.note(format_args!("wrote {}", Count::redirects(count)));
        }
        Ok(())
    }
}

impl Redirects {
    /// A client-side redirect to `target`: a meta-refresh with a canonical link
    /// and a manual fallback anchor. Every value is attribute-escaped by the
    /// markup builder, so no `format!`-built HTML and no bespoke escaper.
    fn stub(target: &str) -> String {
        let mut html = Xml::fragment();
        html.doctype("html");
        html.empty("meta", &[("charset", "utf-8")]);
        html.empty(
            "meta",
            &[
                ("http-equiv", "refresh"),
                ("content", &format!("0; url={target}")),
            ],
        );
        html.empty("link", &[("rel", "canonical"), ("href", target)]);
        html.leaf("title", "Redirecting..");
        html.nest("a", &[("href", target)], |x| x.text("Redirecting.."));
        html.finish()
    }
}
