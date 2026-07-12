//! Syndication feeds: RSS 2.0 and Atom 1.0 from one page set.

use time::OffsetDateTime;
use time::format_description::well_known::{Rfc2822, Rfc3339};

use super::process::{Emit, Processor, Site};
use crate::config::{BaseUrl, Config, FeedKind};
use crate::content::Page;
use crate::engine::xml::Xml;
use crate::error::{FeedDateError, Result};

/// The timestamp behavior each feed standard mandates: RSS wants RFC 2822
/// `pubDate`s, Atom RFC 3339 `updated`s. An inherent extension here (rather
/// than in config) because only the feed writer cares how a kind formats time.
impl FeedKind {
    /// Format a moment as this feed standard requires. Fallible: the formats
    /// have year ranges (RFC 2822: 1900–9999, RFC 3339: 0–9999) a page date
    /// can fall outside of.
    fn timestamp(self, moment: OffsetDateTime) -> Result<String, time::error::Format> {
        match self {
            Self::Rss => moment.format(&Rfc2822),
            Self::Atom => moment.format(&Rfc3339),
        }
    }

    /// The standard's name, for error messages.
    fn standard(self) -> &'static str {
        match self {
            Self::Rss => "RFC 2822",
            Self::Atom => "RFC 3339",
        }
    }
}

/// Emits a syndication feed file per configured format, of the most recent
/// dated pages. Requires a base `url` for the absolute links feeds mandate.
pub(super) struct Feeds;

impl Processor for Feeds {

    fn enabled(&self, config: &Config) -> bool {
        !config.feed.formats.is_empty()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let Some(base) = site.base("feeds", out)? else {
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
        let feed = Feed::new(&base, site.config.label(), &dated);
        for kind in &site.config.feed.formats {
            out.file(&site.config.dist.join(kind.file()), &feed.render(*kind)?)?;
            out.note(format_args!("wrote {}", kind.file()));
        }
        Ok(())
    }
}

/// Renders a feed of the given items (already selected, newest-first) with
/// absolute links under `base`.
pub(super) struct Feed<'a> {
    base: &'a BaseUrl,
    title: &'a str,
    items: &'a [&'a Page],
}

impl<'a> Feed<'a> {
    pub(super) fn new(base: &'a BaseUrl, title: &'a str, items: &'a [&'a Page]) -> Self {
        Self { base, title, items }
    }

    /// Serialize to XML in the requested format. Item timestamps are rendered
    /// up front — the only fallible step — so the XML building itself stays
    /// infallible.
    pub(super) fn render(&self, kind: FeedKind) -> Result<String> {
        let stamps = self.stamps(kind)?;
        let mut xml = Xml::document();
        match kind {
            FeedKind::Rss => self.rss(&mut xml, &stamps),
            FeedKind::Atom => self.atom(&mut xml, &stamps),
        }
        Ok(xml.finish())
    }

    fn home(&self) -> String {
        self.base.home()
    }

    fn link(&self, page: &Page) -> String {
        self.base.join(&page.permalink)
    }

    fn rss(&self, xml: &mut Xml, stamps: &[Option<String>]) {
        xml.nest("rss", &[("version", "2.0")], |xml| {
            xml.nest("channel", &[], |xml| {
                xml.leaf("title", self.title);
                xml.leaf("link", &self.home());
                xml.leaf("description", self.title);
                for (page, stamp) in self.items.iter().zip(stamps) {
                    xml.nest("item", &[], |xml| {
                        let link = self.link(page);
                        if let Some(title) = &page.frontmatter.title {
                            xml.leaf("title", title);
                        }
                        xml.leaf("link", &link);
                        xml.leaf("guid", &link);
                        if let Some(stamp) = stamp {
                            xml.leaf("pubDate", stamp);
                        }
                    });
                }
            });
        });
    }

    fn atom(&self, xml: &mut Xml, stamps: &[Option<String>]) {
        // Items are newest-first, so the first dated one is the feed's `updated`.
        let updated = stamps.iter().flatten().next();
        xml.nest("feed", &[("xmlns", "http://www.w3.org/2005/Atom")], |xml| {
            xml.leaf("title", self.title);
            xml.leaf("id", &self.home());
            xml.empty("link", &[("href", &self.home())]);
            if let Some(updated) = updated {
                xml.leaf("updated", updated);
            }
            for (page, stamp) in self.items.iter().zip(stamps) {
                xml.nest("entry", &[], |xml| {
                    let link = self.link(page);
                    xml.leaf("title", page.frontmatter.title.as_deref().unwrap_or(""));
                    xml.leaf("id", &link);
                    xml.empty("link", &[("href", &link)]);
                    if let Some(stamp) = stamp {
                        xml.leaf("updated", stamp);
                    }
                });
            }
        });
    }

    /// Every item's date rendered in `kind`'s timestamp format (as UTC
    /// midnight), position-aligned with `items`; `None` for undated pages. A
    /// date the format cannot represent is an error, not a silently missing
    /// `pubDate`/`updated`.
    fn stamps(&self, kind: FeedKind) -> Result<Vec<Option<String>>> {
        self.items
            .iter()
            .map(|page| Self::stamp(page, kind))
            .collect()
    }

    fn stamp(page: &Page, kind: FeedKind) -> Result<Option<String>> {
        page.frontmatter
            .date
            .map(|d| {
                kind.timestamp(d.midnight().assume_utc()).map_err(|e| {
                    FeedDateError::new(&page.permalink, d.to_string(), kind.standard(), e).into()
                })
            })
            .transpose()
    }
}
