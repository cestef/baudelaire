//! `sitemap.xml` generation.

use super::process::{Emit, Processor, Site};
use crate::config::{BaseUrl, Config};
use crate::content::Page;
use crate::engine::xml::Xml;
use crate::error::Result;

/// Emits a [sitemaps.org] `sitemap.xml` listing every built page as an absolute
/// URL under the site `base`, with an optional `lastmod` from its date.
/// Requires a base `url` for the absolute URLs the protocol mandates.
///
/// [sitemaps.org]: https://www.sitemaps.org/protocol.html
pub(super) struct SiteMap;

impl SiteMap {
    /// The output file name; robots.txt references it too.
    pub(super) const FILE: &'static str = "sitemap.xml";
    const XMLNS: &'static str = "http://www.sitemaps.org/schemas/sitemap/0.9";

    fn render(base: &BaseUrl, pages: &[Page]) -> String {
        let mut xml = Xml::document();
        xml.nest("urlset", &[("xmlns", Self::XMLNS)], |xml| {
            for page in pages {
                xml.nest("url", &[], |xml| {
                    xml.leaf("loc", &base.join(&page.permalink));
                    if let Some(date) = page.frontmatter.date {
                        // `time::Date` displays as an ISO-8601 calendar date,
                        // exactly the W3C format `lastmod` wants.
                        xml.leaf("lastmod", &date.to_string());
                    }
                });
            }
        });
        xml.finish()
    }
}

impl Processor for SiteMap {
    fn enabled(&self, config: &Config) -> bool {
        config.sitemap
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let Some(base) = site.base("sitemap", out)? else {
            return Ok(());
        };
        out.file(
            &site.config.dist.join(Self::FILE),
            &Self::render(&base, site.pages),
        )?;
        out.note(format_args!("wrote {}", Self::FILE));
        Ok(())
    }
}
