//! Syndication feeds: RSS 2.0, Atom 1.0, and JSON Feed 1.1 from one page set.

use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::{Rfc2822, Rfc3339};

use super::xml::Xml;
use super::{Emit, Processor, Site};
use crate::config::{BaseUrl, Config, FeedKind};
use crate::content::{Page, Taxonomy};
use crate::error::{Artifact, FeedDateError, Result, SerializeError};

/// The timestamp behavior each feed standard mandates: RSS wants RFC 2822
/// `pubDate`s, Atom and JSON Feed RFC 3339. An inherent extension here (rather
/// than in config) because only the feed writer cares how a kind formats time.
impl FeedKind {
    /// Format a moment as this feed standard requires. Fallible: the formats
    /// have year ranges (RFC 2822: 1900–9999, RFC 3339: 0–9999) a page date
    /// can fall outside of.
    fn timestamp(self, moment: OffsetDateTime) -> Result<String, time::error::Format> {
        match self {
            Self::Rss => moment.format(&Rfc2822),
            Self::Atom | Self::Json => moment.format(&Rfc3339),
        }
    }

    /// The standard's name, for error messages.
    fn standard(self) -> &'static str {
        match self {
            Self::Rss => "RFC 2822",
            Self::Atom | Self::Json => "RFC 3339",
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
        let base = site.base("feeds")?;
        // One feed set per language: the default at `/rss.xml`, others under
        // `/{code}/rss.xml`, each listing only its language's recent posts.
        for lang in site.config.langs() {
            let dated = Page::recent(site.pages, lang, site.config.feed.limit);
            let scope = site.config.scope(lang, "");
            let feed = Feed::new(&base, site.config.title(lang), &dated, &scope);
            Self::emit(site, out, &feed)?;
        }
        if site.config.feed.terms {
            self.terms(site, out, &base)?;
        }
        Ok(())
    }
}

impl Feeds {
    /// A feed per taxonomy term, written beside that term's listing page, so a
    /// reader can follow one tag instead of the whole site.
    ///
    /// Terms come from the same grouping that generated those listings, so a
    /// term always has its feed at its own URL and neither can disagree with the
    /// other about which pages belong to it.
    fn terms(&self, site: &Site, out: &mut dyn Emit, base: &BaseUrl) -> Result<()> {
        for group in Taxonomy::groups(site.config, site.pages) {
            let lang = group.lang();
            for term in group.resolve()? {
                let dated = Page::newest(term.members.iter().copied(), site.config.feed.limit);
                // The term's own URL is the feed's home, so its scope is that
                // URL's path: `/fr/tags/rust/` -> `fr/tags/rust`.
                let scope = term.url.trim_matches('/');
                let title = format!("{} - {}", site.config.title(lang), group.title(&term));
                Self::emit(site, out, &Feed::new(base, &title, &dated, scope))?;
            }
        }
        Ok(())
    }

    /// Write a feed in every configured format, beside the page it belongs to.
    /// A feed with nothing dated in it produces no files at all, rather than a
    /// valid but empty one.
    fn emit(site: &Site, out: &mut dyn Emit, feed: &Feed) -> Result<()> {
        if feed.is_empty() {
            return Ok(());
        }
        for kind in &site.config.feed.formats {
            let path = site.config.dist.join(feed.scope).join(kind.file());
            out.file(&path, &feed.render(*kind)?)?;
            out.note(format_args!("wrote {}", path.display()));
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
    /// This feed's language path segment (empty for the default language).
    ///
    /// Every feed used to advertise the site root as its `<link>` and `<id>`.
    /// Atom requires a unique feed id, so to an aggregator `/rss.xml` and
    /// `/fr/rss.xml` were one feed with two sets of entries.
    scope: &'a str,
}

impl<'a> Feed<'a> {
    pub(super) fn new(
        base: &'a BaseUrl,
        title: &'a str,
        items: &'a [&'a Page],
        scope: &'a str,
    ) -> Self {
        Self {
            base,
            title,
            items,
            scope,
        }
    }

    /// Whether this feed has nothing to syndicate.
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Serialize in the requested format. Item timestamps are rendered up
    /// front: for XML the only fallible step, so the building itself stays
    /// infallible.
    pub(super) fn render(&self, kind: FeedKind) -> Result<String> {
        let stamps = self.stamps(kind)?;
        match kind {
            FeedKind::Rss => Ok(self.xml(Self::rss, &stamps)),
            FeedKind::Atom => Ok(self.xml(Self::atom, &stamps)),
            FeedKind::Json => self.json(&stamps),
        }
    }

    /// An XML document written by one format's channel writer.
    fn xml(
        &self,
        write: fn(&Self, &mut Xml, &[Option<String>]),
        stamps: &[Option<String>],
    ) -> String {
        let mut xml = Xml::document();
        write(self, &mut xml, stamps);
        xml.finish()
    }

    /// This feed's language home, the site root for the default language.
    fn home(&self) -> String {
        match self.scope.is_empty() {
            true => self.base.home(),
            false => self.base.join(format!("/{}/", self.scope)),
        }
    }

    /// This feed's own absolute URL, its stable identity.
    fn url(&self, kind: FeedKind) -> String {
        format!("{}{}", self.home(), kind.file())
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
                        xml.leaf("title", page.title());
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
            xml.leaf("id", &self.url(FeedKind::Atom));
            xml.empty("link", &[("href", &self.home())]);
            if let Some(updated) = updated {
                xml.leaf("updated", updated);
            }
            for (page, stamp) in self.items.iter().zip(stamps) {
                xml.nest("entry", &[], |xml| {
                    let link = self.link(page);
                    xml.leaf("title", page.title());
                    xml.leaf("id", &link);
                    xml.empty("link", &[("href", &link)]);
                    if let Some(stamp) = stamp {
                        xml.leaf("updated", stamp);
                    }
                });
            }
        });
    }

    /// The JSON Feed 1.1 document (https://jsonfeed.org/version/1.1).
    fn json(&self, stamps: &[Option<String>]) -> Result<String> {
        let feed = JsonFeed {
            version: "https://jsonfeed.org/version/1.1",
            title: self.title,
            home_page_url: self.home(),
            feed_url: self.url(FeedKind::Json),
            items: self
                .items
                .iter()
                .zip(stamps)
                .map(|(page, stamp)| {
                    let link = self.link(page);
                    JsonItem {
                        id: link.clone(),
                        url: link,
                        title: Some(page.title()),
                        date_published: stamp.as_deref(),
                    }
                })
                .collect(),
        };
        serde_json::to_string(&feed).map_err(|e| SerializeError::new(Artifact::Feed, e).into())
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

/// The JSON Feed 1.1 top-level object: just the required members plus the
/// item list; optional members are omitted, not emitted empty.
#[derive(Serialize)]
struct JsonFeed<'a> {
    version: &'static str,
    title: &'a str,
    home_page_url: String,
    feed_url: String,
    items: Vec<JsonItem<'a>>,
}

/// One JSON Feed item. `id` doubles as the canonical `url`, mirroring the
/// `guid`/`link` pairing in the XML formats.
#[derive(Serialize)]
struct JsonItem<'a> {
    id: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_published: Option<&'a str>,
}
