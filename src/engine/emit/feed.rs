//! Syndication feeds: RSS 2.0, Atom 1.0, and JSON Feed 1.1 from one page set.

use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::{Rfc2822, Rfc3339};

use super::xml::Xml;
use super::{Emit, Processor, Site, Warn};
use crate::config::{BaseUrl, Channel, Config, FeedConfig, FeedKind, Permalink};
use crate::content::{Page, Taxonomy};
use crate::error::warning::FeedMounted;
use crate::error::{Artifact, FeedDateError, Result};

/// The timestamp behavior each feed standard mandates: RFC 2822 for RSS,
/// RFC 3339 for Atom and JSON Feed.
impl FeedKind {
    /// Format a moment as this feed standard requires, fallible since the
    /// formats have year ranges (RFC 2822: 1900-9999, RFC 3339: 0-9999) a page
    /// date can fall outside of.
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

/// Each page's prose, as the markup a full-content feed carries, keyed by
/// permalink.
///
/// Empty for a summary feed, which is what makes [`Feed::body`] a lookup
/// rather than a branch: nothing is in the map, so nothing is found.
#[derive(Default)]
struct Bodies<'a>(std::collections::HashMap<&'a str, &'a str>);

impl<'a> Bodies<'a> {
    /// The prose of every built page, or nothing at all when the site asked
    /// for summaries.
    fn of(site: &'a Site<'a>) -> Self {
        Self(
            site.outputs
                .iter()
                .filter_map(|out| {
                    let body = out.syndicated?;
                    Some((out.page.permalink.as_str(), body.0.as_str()))
                })
                .collect(),
        )
    }

    /// This page's prose, or `None` when the site asked for summaries or the
    /// page was not built in this run.
    fn get(&self, page: &Page) -> Option<&'a str> {
        self.0.get(page.permalink.as_str()).copied()
    }
}

/// What every feed on one site shares.
#[derive(Clone, Copy)]
struct Shared<'a> {
    /// The site's config, for resolving a page's byline.
    config: &'a Config,
    /// Where the site is served, for the absolute links every format mandates.
    base: &'a BaseUrl,
    /// The `feed { }` config, for what each format's file is called.
    names: &'a FeedConfig,
    /// Each page's prose, when the site asked its feeds to carry it.
    bodies: &'a Bodies<'a>,
    /// The site's own `author`, for Atom's mandatory feed-level `<author>`.
    author: Option<&'a str>,
    /// Who is behind a page, so an entry names its own people rather than the
    /// site's one name.
    entities: &'a crate::content::Registries,
}

/// Emits a syndication feed file per configured format, of the most recent
/// dated pages. Requires a base `url` for the absolute links feeds mandate.
pub(super) struct Feeds;

impl Processor for Feeds {
    fn enabled(&self, config: &Config) -> bool {
        !config.generate.feed.formats.is_empty()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let base = site.base("feeds")?;
        let bodies = Bodies::of(site);
        let shared = Shared {
            config: site.config,
            base: &base,
            names: &site.config.generate.feed,
            bodies: &bodies,
            author: site.config.author.as_deref(),
            entities: site.entities,
        };
        for lang in site.config.langs() {
            let dated = Page::recent(
                site.pages,
                site.config,
                lang,
                site.config.generate.feed.limit,
                None,
            );
            let scope = site.config.scope(lang, "");
            let feed = Feed::new(
                shared,
                site.config.title(lang),
                site.config.description(lang),
                &dated,
                &scope,
            );
            Self::emit(site, out, &feed, Empty::Written)?;
        }
        Self::collections(site, out, shared)?;
        if site.config.generate.feed.terms {
            Self::terms(site, out, shared)?;
        }
        Ok(())
    }
}

impl Feeds {
    /// A feed per collection that asked for one, written beside that
    /// collection's index.
    ///
    /// Where each one goes and whether it has one at all is
    /// [`Config::channel`], the same answer the `<head>` tag advertising it is
    /// built from, so a page can never point at a file this pass declined to
    /// write.
    fn collections(site: &Site, out: &mut dyn Emit, shared: Shared<'_>) -> Result<()> {
        for (id, collection) in &site.config.content.collections {
            if !collection.feed || !collection.paginate.enabled {
                continue;
            }
            let channels: Vec<(&str, Channel)> = site
                .config
                .langs()
                .into_iter()
                .filter_map(|lang| Some((lang, site.config.channel(id, lang)?)))
                .collect();
            if channels.is_empty() {
                out.warn(FeedMounted {
                    collection: id.clone(),
                });
                continue;
            }
            for (lang, channel) in channels {
                let dated = Page::recent(
                    site.pages,
                    site.config,
                    lang,
                    site.config.generate.feed.limit,
                    Some(id),
                );
                Self::emit(
                    site,
                    out,
                    &Feed::new(
                        shared,
                        &channel.title,
                        site.config.description(lang),
                        &dated,
                        &channel.scope,
                    ),
                    Empty::Written,
                )?;
            }
        }
        Ok(())
    }

    /// A feed per taxonomy term, written beside that term's listing page, from
    /// the same grouping that generated the listing.
    fn terms(site: &Site, out: &mut dyn Emit, shared: Shared<'_>) -> Result<()> {
        for group in Taxonomy::groups(site.config, site.entities, site.pages) {
            let lang = group.lang();
            for term in group.resolve()? {
                let dated = Page::newest(
                    term.members.iter().copied(),
                    site.config.generate.feed.limit,
                );
                let scope = term.url.trim_matches('/');
                let title = format!("{} - {}", site.config.title(lang), group.title(&term));
                Self::emit(
                    site,
                    out,
                    &Feed::new(shared, &title, site.config.description(lang), &dated, scope),
                    Empty::Skipped,
                )?;
            }
        }
        Ok(())
    }

    /// Write a feed in every configured format, beside the page it belongs to.
    fn emit(site: &Site, out: &mut dyn Emit, feed: &Feed, empty: Empty) -> Result<()> {
        if feed.is_empty() && empty == Empty::Skipped {
            return Ok(());
        }
        for kind in &site.config.generate.feed.formats {
            let path = site.dist(&[feed.scope, site.config.generate.feed.file(*kind)]);
            out.file(&path, &feed.render(*kind)?)?;
            out.wrote(&path);
        }
        Ok(())
    }
}

/// Whether a feed with nothing dated in it is still written, which follows
/// from whether anything advertises the file.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Empty {
    /// Written anyway, because a page advertises it.
    Written,
    /// Skipped, because nothing does.
    Skipped,
}

/// Renders a feed of the given items (already selected, newest-first) with
/// absolute links under `base`.
struct Feed<'a> {
    site: Shared<'a>,
    title: &'a str,
    /// What the site is, from `description` in the feed's own language; RSS
    /// makes the channel element mandatory, so unset it falls back to the
    /// title.
    description: Option<&'a str>,
    items: &'a [&'a Page],
    /// This feed's language path segment, empty for the default language.
    scope: &'a str,
}

impl<'a> Feed<'a> {
    fn new(
        site: Shared<'a>,
        title: &'a str,
        description: Option<&'a str>,
        items: &'a [&'a Page],
        scope: &'a str,
    ) -> Self {
        Self {
            site,
            title,
            description,
            items,
            scope,
        }
    }

    /// This page's prose as a feed entry carries it, or `None` for a summary
    /// feed.
    fn body(&self, page: &Page) -> Option<&'a str> {
        self.site.bodies.get(page)
    }

    /// The feed's own blurb: the configured description, else its title.
    fn blurb(&self) -> &str {
        self.description.unwrap_or(self.title)
    }

    /// Whether this feed has nothing to syndicate.
    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Serialize in the requested format; item timestamps are rendered up
    /// front, being the only fallible step.
    fn render(&self, kind: FeedKind) -> Result<String> {
        let stamps = self.stamps(kind)?;
        match kind {
            FeedKind::Rss => Ok(self.xml(Self::rss, &stamps)),
            FeedKind::Atom => Ok(self.xml(Self::atom, &stamps)),
            FeedKind::Json => self.json(&stamps),
        }
    }

    /// An XML document written by one format's channel writer.
    fn xml(&self, write: fn(&Self, &mut Xml, &[Stamps]), stamps: &[Stamps]) -> String {
        let mut xml = Xml::document();
        write(self, &mut xml, stamps);
        xml.finish()
    }

    /// This feed's language home, the site root for the default language.
    fn home(&self) -> String {
        self.site.base.join(Permalink::join(&[self.scope]))
    }

    /// This feed's own absolute URL, its stable identity.
    fn url(&self, kind: FeedKind) -> String {
        self.site.names.url(kind, self.site.base, self.scope)
    }

    fn link(&self, page: &Page) -> String {
        self.site.base.join(&page.permalink)
    }

    /// The namespace `content:encoded` lives in, declared on `<rss>` whenever
    /// a full feed is written: an undeclared prefix is not well-formed XML.
    const CONTENT_NS: (&'static str, &'static str) =
        ("xmlns:content", "http://purl.org/rss/1.0/modules/content/");

    fn rss(&self, xml: &mut Xml, stamps: &[Stamps]) {
        let mut attrs: Vec<(&str, &str)> = vec![("version", "2.0")];
        if self.site.names.full() {
            attrs.push(Self::CONTENT_NS);
        }
        xml.nest("rss", &attrs, |xml| {
            xml.nest("channel", &[], |xml| {
                xml.leaf("title", self.title);
                xml.leaf("link", &self.home());
                xml.leaf("description", self.blurb());
                for (page, stamp) in self.items.iter().zip(stamps) {
                    xml.nest("item", &[], |xml| {
                        let link = self.link(page);
                        xml.leaf("title", page.title());
                        xml.leaf("link", &link);
                        xml.leaf("guid", &link);
                        if let Some(description) = page.frontmatter.blurb() {
                            xml.leaf("description", description);
                        }
                        if let Some(body) = self.body(page) {
                            xml.leaf("content:encoded", body);
                        }
                        for term in Self::categories(page) {
                            xml.leaf("category", term);
                        }
                        if let Some(published) = &stamp.published {
                            xml.leaf("pubDate", published);
                        }
                    });
                }
            });
        });
    }

    /// The people one entry credits, as Atom writes a person: a name, and the
    /// two optional children it allows beside it.
    ///
    /// Falls back to nothing rather than to the site's author, which the feed
    /// itself already carries.
    fn people(&self, xml: &mut Xml, page: &Page) {
        let byline = crate::content::Byline::of(self.site.entities, self.site.config, page);
        for (role, credited) in byline.roles() {
            let Some(element) = role.spelling(crate::content::entities::Vocabulary::Atom) else {
                continue;
            };
            for one in credited {
                xml.nest(element, &[], |xml| {
                    xml.leaf("name", &one.display);
                    if let Some(url) = &one.url {
                        xml.leaf("uri", url);
                    }
                    if let Some(email) = &one.email {
                        xml.leaf("email", email);
                    }
                });
            }
        }
    }

    fn atom(&self, xml: &mut Xml, stamps: &[Stamps]) {
        let updated = stamps.iter().find_map(Stamps::latest);
        xml.nest("feed", &[("xmlns", "http://www.w3.org/2005/Atom")], |xml| {
            xml.leaf("title", self.title);
            if let Some(description) = self.description {
                xml.leaf("subtitle", description);
            }
            xml.leaf("id", &self.url(FeedKind::Atom));
            xml.empty("link", &[("href", &self.home())]);
            xml.empty(
                "link",
                &[("rel", "self"), ("href", &self.url(FeedKind::Atom))],
            );
            if let Some(author) = self.site.author {
                xml.nest("author", &[], |xml| xml.leaf("name", author));
            }
            if let Some(updated) = updated {
                xml.leaf("updated", updated);
            }
            for (page, stamp) in self.items.iter().zip(stamps) {
                xml.nest("entry", &[], |xml| {
                    let link = self.link(page);
                    xml.leaf("title", page.title());
                    xml.leaf("id", &link);
                    xml.empty("link", &[("href", &link)]);
                    if let Some(updated) = stamp.latest() {
                        xml.leaf("updated", updated);
                    }
                    if let Some(published) = &stamp.published {
                        xml.leaf("published", published);
                    }
                    self.people(xml, page);
                    if let Some(description) = page.frontmatter.blurb() {
                        xml.leaf("summary", description);
                    }
                    if let Some(body) = self.body(page) {
                        xml.tagged("content", &[("type", "html")], body);
                    }
                    for term in Self::categories(page) {
                        xml.empty("category", &[("term", term)]);
                    }
                });
            }
        });
    }

    /// Every taxonomy term on a page, flattened: a feed's categories are a flat
    /// keyword list in all three standards, with nowhere to say which taxonomy
    /// a term came from.
    fn categories(page: &Page) -> impl Iterator<Item = &str> {
        page.frontmatter
            .taxonomies
            .values()
            .flatten()
            .map(String::as_str)
    }

    /// The JSON Feed 1.1 document (https://jsonfeed.org/version/1.1).
    fn json(&self, stamps: &[Stamps]) -> Result<String> {
        let feed = JsonFeed {
            version: "https://jsonfeed.org/version/1.1",
            title: self.title,
            description: self.description,
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
                        summary: page.frontmatter.blurb().map(str::to_owned),
                        content_html: self.body(page),
                        date_published: stamp.published.as_deref(),
                        date_modified: stamp.updated.as_deref(),
                        tags: Self::categories(page).collect(),
                    }
                })
                .collect(),
        };
        Artifact::Feed.json(&feed)
    }

    /// Every item's dates rendered in `kind`'s timestamp format (as UTC
    /// midnight), position-aligned with `items`. A date the format cannot
    /// represent is an error, not a silently missing `pubDate`/`updated`.
    fn stamps(&self, kind: FeedKind) -> Result<Vec<Stamps>> {
        self.items
            .iter()
            .map(|page| {
                Ok(Stamps {
                    published: Self::stamp(page, page.frontmatter.date, kind)?,
                    updated: Self::stamp(page, page.frontmatter.updated, kind)?,
                })
            })
            .collect()
    }

    fn stamp(page: &Page, date: Option<time::Date>, kind: FeedKind) -> Result<Option<String>> {
        date.map(|d| {
            kind.timestamp(d.midnight().assume_utc()).map_err(|e| {
                FeedDateError::new(&page.permalink, d.to_string(), kind.standard(), e).into()
            })
        })
        .transpose()
    }
}

/// One item's two moments, each already in its feed's format: `published` is
/// when the page went up and orders the feed, `updated` when it last changed.
struct Stamps {
    published: Option<String>,
    updated: Option<String>,
}

impl Stamps {
    /// The moment that dates this item at all: its `updated` if it has one,
    /// else when it was published.
    fn latest(&self) -> Option<&String> {
        self.updated.as_ref().or(self.published.as_ref())
    }
}

/// The JSON Feed 1.1 top-level object; optional members are omitted, not
/// emitted empty.
#[derive(Serialize)]
struct JsonFeed<'a> {
    version: &'static str,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    home_page_url: String,
    feed_url: String,
    items: Vec<JsonItem<'a>>,
}

/// One JSON Feed item, whose `id` doubles as the canonical `url`.
#[derive(Serialize)]
struct JsonItem<'a> {
    id: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    /// The entry's prose, when the site asked its feeds to carry it.
    #[serde(skip_serializing_if = "Option::is_none")]
    content_html: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_published: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_modified: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<&'a str>,
}
