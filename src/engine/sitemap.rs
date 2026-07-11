//! `sitemap.xml` generation.

use super::process::{Emit, Processor, Site};
use crate::config::{BaseUrl, Config};
use crate::content::Page;
use crate::engine::xml::Xml;
use crate::error::Result;

/// Emits `sitemap.xml`. Requires a base `url` for the absolute URLs the
/// sitemaps protocol mandates.
pub(super) struct SiteMap;

impl SiteMap {
    /// The output file name — robots.txt references it too.
    pub(super) const FILE: &'static str = "sitemap.xml";
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
            &Sitemap::new(&base, site.pages).render()?,
        )?;
        out.note(format_args!("wrote {}", Self::FILE))
    }
}

/// Renders a [sitemaps.org] `sitemap.xml` listing every built page as an
/// absolute URL under `base`, with an optional `lastmod` from its date.
///
/// [sitemaps.org]: https://www.sitemaps.org/protocol.html
pub(super) struct Sitemap<'a> {
    base: &'a BaseUrl,
    pages: &'a [Page],
}

impl<'a> Sitemap<'a> {
    const XMLNS: &'static str = "http://www.sitemaps.org/schemas/sitemap/0.9";

    pub(super) fn new(base: &'a BaseUrl, pages: &'a [Page]) -> Self {
        Self { base, pages }
    }

    /// The serialized XML.
    pub(super) fn render(&self) -> Result<String> {
        let mut xml = Xml::document()?;
        xml.nest("urlset", &[("xmlns", Self::XMLNS)], |xml| {
            for page in self.pages {
                xml.nest("url", &[], |xml| {
                    xml.leaf("loc", &self.base.join(&page.permalink))?;
                    if let Some(date) = page.frontmatter.date {
                        // `time::Date` displays as an ISO-8601 calendar date,
                        // exactly the W3C format `lastmod` wants.
                        xml.leaf("lastmod", &date.to_string())?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })?;
        xml.finish()
    }
}
