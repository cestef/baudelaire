//! `sitemap.xml` generation.

use std::path::{Path, PathBuf};

use super::xml::Xml;
use super::{Emit, Processor, Reads, Site};
use crate::config::{BaseUrl, Config};
use crate::content::page::Translation;
use crate::content::{Generated, Page, Relations};
use crate::error::Result;
use crate::git::History;

/// Emits a [sitemaps.org] `sitemap.xml` listing every built page as an absolute
/// URL under the site `base`, with an optional `lastmod`.
///
/// [sitemaps.org]: https://www.sitemaps.org/protocol.html
pub(super) struct SiteMap;

impl SiteMap {
    pub(super) const FILE: &'static str = "sitemap.xml";
    const XMLNS: &'static str = "http://www.sitemaps.org/schemas/sitemap/0.9";
    const XHTML: &'static str = "http://www.w3.org/1999/xhtml";

    /// When a page last changed, as `lastmod` reports it.
    ///
    /// An author who wrote `updated` has said so and is believed. Otherwise the
    /// repository knows, and knows better than `date`, which is when the page
    /// was *published*: the two differ on every page edited after the fact, and
    /// `date` is what a crawler was told last time.
    fn modified(page: &Page, history: &History, root: &Path) -> Option<String> {
        if let Some(updated) = page.frontmatter.updated {
            return Some(updated.to_string());
        }
        let key = crate::graph::Portable(root).key(&page.source);
        history
            .of(&key)
            .map(|changed| changed.committed().to_owned())
            .or_else(|| page.frontmatter.date.map(|date| date.to_string()))
    }

    fn render(
        base: &BaseUrl,
        pages: &[Page],
        relations: &Relations,
        config: &Config,
        history: &History,
        root: &Path,
    ) -> String {
        let mut xml = Xml::document();
        let ns: &[(&str, &str)] = &[("xmlns", Self::XMLNS), ("xmlns:xhtml", Self::XHTML)];
        xml.nest("urlset", ns, |xml| {
            for page in pages
                .iter()
                .filter(|p| p.listed(config) && !p.frontmatter.excludes(Generated::Sitemap))
            {
                xml.nest("url", &[], |xml| {
                    xml.leaf("loc", &base.join(&page.permalink));
                    if let Some(modified) = Self::modified(page, history, root) {
                        xml.leaf("lastmod", &modified);
                    }
                    Self::alternates(xml, base, &relations.of(page).translations, config);
                });
            }
        });
        xml.finish()
    }

    /// The `hreflang` alternates for a translated page: one per edition plus an
    /// `x-default` pointing at the default language's. A single-language page
    /// has no translations and emits none.
    fn alternates(xml: &mut Xml, base: &BaseUrl, translations: &[Translation], config: &Config) {
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
        for t in translations {
            link(xml, &t.lang, &t.url);
        }
        if let Some(default) = translations.iter().find(|t| t.lang == config.lang) {
            link(xml, "x-default", &default.url);
        }
    }
}

impl Processor for SiteMap {
    fn name(&self) -> &'static str {
        "the sitemap"
    }

    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        vec![Site::at(config, &[Self::FILE])]
    }

    fn inputs(&self, _config: &Config) -> Option<&'static [Reads]> {
        Some(&[Reads::Listing, Reads::Relations, Reads::History])
    }

    fn enabled(&self, config: &Config) -> bool {
        config.generate.sitemap
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let base = site.base("sitemap")?;
        let path = site.dist(&[Self::FILE]);
        out.file(
            &path,
            &Self::render(
                &base,
                site.pages,
                site.relations,
                site.config,
                site.history,
                &site.config.root,
            ),
        )?;
        out.wrote(&path);
        Ok(())
    }
}
