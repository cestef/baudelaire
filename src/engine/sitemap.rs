//! `sitemap.xml` generation.

use super::process::{Emit, Processor, Site};
use crate::config::Config;
use crate::content::Page;
use crate::engine::xml::Xml;
use crate::error::Result;

/// Emits `sitemap.xml`. Requires a base `url` for the absolute URLs the
/// sitemaps protocol mandates.
pub(super) struct SiteMap;

impl Processor for SiteMap {

    fn enabled(&self, config: &Config) -> bool {
        config.sitemap
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let Some(base) = site.base_url() else {
            out.warn(format_args!("sitemap enabled but no `url` set — skipped"))?;
            return Ok(());
        };
        out.file(
            &site.config.dist.join("sitemap.xml"),
            &Sitemap::new(base, site.pages).render()?,
        )?;
        out.note(format_args!("wrote sitemap.xml"))
    }
}

/// Renders a [sitemaps.org] `sitemap.xml` listing every built page as an
/// absolute URL under `base`, with an optional `lastmod` from its date.
///
/// [sitemaps.org]: https://www.sitemaps.org/protocol.html
pub(super) struct Sitemap<'a> {
    base: &'a str,
    pages: &'a [Page],
}

impl<'a> Sitemap<'a> {
    const XMLNS: &'static str = "http://www.sitemaps.org/schemas/sitemap/0.9";

    pub(super) fn new(base: &'a str, pages: &'a [Page]) -> Self {
        Self {
            base: base.trim_end_matches('/'),
            pages,
        }
    }

    /// The serialized XML.
    pub(super) fn render(&self) -> Result<String> {
        let mut xml = Xml::document()?;
        xml.nest("urlset", &[("xmlns", Self::XMLNS)], |xml| {
            for page in self.pages {
                xml.nest("url", &[], |xml| {
                    xml.leaf("loc", &format!("{}{}", self.base, page.permalink))?;
                    if let Some(date) = page.frontmatter.date {
                        let lastmod = format!(
                            "{:04}-{:02}-{:02}",
                            date.year(),
                            u8::from(date.month()),
                            date.day()
                        );
                        xml.leaf("lastmod", &lastmod)?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })?;
        xml.finish()
    }
}
