//! Syndication feeds: RSS 2.0 and Atom 1.0 from one page set.

use time::format_description::well_known::{Rfc2822, Rfc3339};

use super::process::{Emit, Processor, Site};
use crate::config::{Config, FeedKind};
use crate::content::Page;
use crate::engine::xml::Xml;
use crate::error::Result;

/// Emits a syndication feed file per configured format, of the most recent
/// dated pages. Requires a base `url` for the absolute links feeds mandate.
pub(super) struct Feeds;

impl Processor for Feeds {
    fn enabled(&self, config: &Config) -> bool {
        !config.feed.formats.is_empty()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let Some(base) = site.base_url() else {
            out.warn(format_args!("feeds enabled but no `url` set — skipped"))?;
            return Ok(());
        };
        let mut dated: Vec<&Page> = site
            .pages
            .iter()
            .filter(|p| p.frontmatter.date.is_some())
            .collect();
        dated.sort_by_key(|p| std::cmp::Reverse(p.frontmatter.date));
        dated.truncate(site.config.feed.limit);
        if dated.is_empty() {
            return Ok(());
        }
        let feed = Feed::new(base, site.config.label(), &dated);
        for kind in &site.config.feed.formats {
            out.file(&site.config.dist.join(kind.file()), &feed.render(*kind)?)?;
            out.note(format_args!("wrote {}", kind.file()))?;
        }
        Ok(())
    }
}

/// Renders a feed of the given items (already selected, newest-first) with
/// absolute links under `base`.
pub(super) struct Feed<'a> {
    base: &'a str,
    title: &'a str,
    items: &'a [&'a Page],
}

impl<'a> Feed<'a> {
    pub(super) fn new(base: &'a str, title: &'a str, items: &'a [&'a Page]) -> Self {
        Self {
            base: base.trim_end_matches('/'),
            title,
            items,
        }
    }

    /// Serialize to XML in the requested format.
    pub(super) fn render(&self, kind: FeedKind) -> Result<String> {
        let mut xml = Xml::document()?;
        match kind {
            FeedKind::Rss => self.rss(&mut xml)?,
            FeedKind::Atom => self.atom(&mut xml)?,
        }
        xml.finish()
    }

    fn home(&self) -> String {
        format!("{}/", self.base)
    }

    fn link(&self, page: &Page) -> String {
        format!("{}{}", self.base, page.permalink)
    }

    fn rss(&self, xml: &mut Xml) -> std::io::Result<()> {
        xml.nest("rss", &[("version", "2.0")], |xml| {
            xml.nest("channel", &[], |xml| {
                xml.leaf("title", self.title)?;
                xml.leaf("link", &self.home())?;
                xml.leaf("description", self.title)?;
                for page in self.items {
                    xml.nest("item", &[], |xml| {
                        let link = self.link(page);
                        if let Some(title) = &page.frontmatter.title {
                            xml.leaf("title", title)?;
                        }
                        xml.leaf("link", &link)?;
                        xml.leaf("guid", &link)?;
                        if let Some(stamp) = Self::stamp(page, &Rfc2822) {
                            xml.leaf("pubDate", &stamp)?;
                        }
                        Ok(())
                    })?;
                }
                Ok(())
            })
        })
    }

    fn atom(&self, xml: &mut Xml) -> std::io::Result<()> {
        let updated = self.items.iter().find_map(|p| Self::stamp(p, &Rfc3339));
        xml.nest("feed", &[("xmlns", "http://www.w3.org/2005/Atom")], |xml| {
            xml.leaf("title", self.title)?;
            xml.leaf("id", &self.home())?;
            xml.empty("link", &[("href", &self.home())])?;
            if let Some(updated) = &updated {
                xml.leaf("updated", updated)?;
            }
            for page in self.items {
                xml.nest("entry", &[], |xml| {
                    let link = self.link(page);
                    xml.leaf("title", page.frontmatter.title.as_deref().unwrap_or(""))?;
                    xml.leaf("id", &link)?;
                    xml.empty("link", &[("href", &link)])?;
                    if let Some(stamp) = Self::stamp(page, &Rfc3339) {
                        xml.leaf("updated", &stamp)?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })
    }

    /// A page's date formatted with `fmt` (as UTC midnight), if it has one.
    fn stamp(page: &Page, fmt: &(impl time::formatting::Formattable + ?Sized)) -> Option<String> {
        page.frontmatter
            .date
            .and_then(|d| d.midnight().assume_utc().format(fmt).ok())
    }
}
