//! `sitemap.xml` generation.

use super::xml::Xml;
use super::{Emit, Processor, Site};
use crate::config::{BaseUrl, Config};
use crate::content::Page;
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
    const XHTML: &'static str = "http://www.w3.org/1999/xhtml";

    fn render(base: &BaseUrl, pages: &[Page], config: &Config) -> String {
        let mut xml = Xml::document();
        let ns: &[(&str, &str)] = &[("xmlns", Self::XMLNS), ("xmlns:xhtml", Self::XHTML)];
        xml.nest("urlset", ns, |xml| {
            for page in pages {
                xml.nest("url", &[], |xml| {
                    xml.leaf("loc", &base.join(&page.permalink));
                    if let Some(date) = page.frontmatter.modified() {
                        // `time::Date` displays as an ISO-8601 calendar date,
                        // exactly the W3C format `lastmod` wants.
                        xml.leaf("lastmod", &date.to_string());
                    }
                    Self::alternates(xml, base, page, config);
                });
            }
        });
        xml.finish()
    }

    /// The `hreflang` alternates for a translated page: one per edition plus an
    /// `x-default` pointing at the default language's. A single-language page
    /// has no translations and emits none.
    fn alternates(xml: &mut Xml, base: &BaseUrl, page: &Page, config: &Config) {
        let link = |xml: &mut Xml, hreflang: &str, url: &str| {
            let href = base.join(url);
            xml.empty(
                "xhtml:link",
                &[
                    ("rel", "alternate"),
                    ("hreflang", hreflang),
                    ("href", &href),
                ],
            );
        };
        for t in &page.translations {
            link(xml, &t.lang, &t.url);
        }
        if let Some(default) = page.translations.iter().find(|t| t.lang == config.lang) {
            link(xml, "x-default", &default.url);
        }
    }
}

impl Processor for SiteMap {
    fn enabled(&self, config: &Config) -> bool {
        config.generate.sitemap
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let base = site.base("sitemap")?;
        out.file(
            &site.dist(&[Self::FILE]),
            &Self::render(&base, site.pages, site.config),
        )?;
        out.note(format_args!("wrote {}", Self::FILE));
        Ok(())
    }
}
