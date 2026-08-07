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

/// Each page's prose, as the markup a full-content feed carries.
///
/// Built once per run and keyed by permalink, because every feed a site writes
/// draws from the same pages: a site feed, one per collection and one per term
/// all list the same posts, and slicing each page's region again per feed is the
/// same work three times.
///
/// Empty for a summary feed, which is what makes `Feed::body` a lookup rather
/// than a branch: nothing is in the map, so nothing is found.
#[derive(Default)]
struct Bodies<'a>(std::collections::HashMap<&'a str, &'a str>);

impl<'a> Bodies<'a> {
    /// The prose of every built page, or nothing at all when the site asked for
    /// summaries.
    ///
    /// A lookup rather than work: the markup was captured off the typed DOM at
    /// compile time and replayed here for a cached page just the same, so this
    /// only indexes it by the permalink a feed entry knows the page by. Empty
    /// for a summary feed, which is what makes [`Feed::body`] a lookup rather
    /// than a branch: nothing is in the map, so nothing is found.
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
        // One feed set per language: the default at `/rss.xml`, others under
        // `/{code}/rss.xml`, each listing only its language's recent posts.
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
                &base,
                site.config.title(lang),
                site.config.description(lang),
                &dated,
                &scope,
                &site.config.generate.feed,
                &bodies,
            );
            Self::emit(site, out, &feed)?;
        }
        Self::collections(site, out, &base, &bodies)?;
        if site.config.generate.feed.terms {
            Self::terms(site, out, &base, &bodies)?;
        }
        Ok(())
    }
}

impl Feeds {
    /// A feed per collection that asked for one, written beside that
    /// collection's index, so a reader can follow the essays without also
    /// taking the release notes.
    ///
    /// Where each one goes, what it calls itself, and whether it has one at
    /// all is [`Config::channel`]: the same answer the `<head>` tag advertising
    /// it is built from, so a page can never point at a file this pass declined
    /// to write, nor name it something else.
    fn collections(site: &Site, out: &mut dyn Emit, base: &BaseUrl, bodies: &Bodies) -> Result<()> {
        for (id, collection) in &site.config.content.collections {
            // The `paginate` half is what `engine::gate` warns about, and the
            // warning states that no feed is written: it has to be true here or
            // the diagnostic describes a build that did not happen.
            if !collection.feed || !collection.paginate.enabled {
                continue;
            }
            let channels: Vec<(&str, Channel)> = site
                .config
                .langs()
                .into_iter()
                .filter_map(|lang| Some((lang, site.config.channel(id, lang)?)))
                .collect();
            // Asked for, able to have one, and yet no language places it
            // anywhere: the collection sits where a site feed already does, and
            // that file is taken. Said once, since the mount is one config line.
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
                        base,
                        &channel.title,
                        site.config.description(lang),
                        &dated,
                        &channel.scope,
                        &site.config.generate.feed,
                        bodies,
                    ),
                )?;
            }
        }
        Ok(())
    }

    /// A feed per taxonomy term, written beside that term's listing page, so a
    /// reader can follow one tag instead of the whole site.
    ///
    /// Terms come from the same grouping that generated those listings, so a
    /// term always has its feed at its own URL and neither can disagree with the
    /// other about which pages belong to it.
    fn terms(site: &Site, out: &mut dyn Emit, base: &BaseUrl, bodies: &Bodies) -> Result<()> {
        for group in Taxonomy::groups(site.config, site.pages) {
            let lang = group.lang();
            for term in group.resolve()? {
                let dated = Page::newest(
                    term.members.iter().copied(),
                    site.config.generate.feed.limit,
                );
                // The term's own URL is the feed's home, so its scope is that
                // URL's path: `/fr/tags/rust/` -> `fr/tags/rust`.
                let scope = term.url.trim_matches('/');
                let title = format!("{} - {}", site.config.title(lang), group.title(&term));
                Self::emit(
                    site,
                    out,
                    &Feed::new(
                        base,
                        &title,
                        site.config.description(lang),
                        &dated,
                        scope,
                        &site.config.generate.feed,
                        bodies,
                    ),
                )?;
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
        for kind in &site.config.generate.feed.formats {
            let path = site.dist(&[feed.scope, site.config.generate.feed.file(*kind)]);
            out.file(&path, &feed.render(*kind)?)?;
            out.wrote(&path);
        }
        Ok(())
    }
}

/// Renders a feed of the given items (already selected, newest-first) with
/// absolute links under `base`.
struct Feed<'a> {
    base: &'a BaseUrl,
    title: &'a str,
    /// What the site is, from `description` in the feed's own language. RSS
    /// makes the channel element mandatory, so unset it falls back to the
    /// title, which is what every feed said before there was a key for it.
    description: Option<&'a str>,
    items: &'a [&'a Page],
    /// This feed's language path segment (empty for the default language).
    ///
    /// Every feed used to advertise the site root as its `<link>` and `<id>`.
    /// Atom requires a unique feed id, so to an aggregator `/rss.xml` and
    /// `/fr/rss.xml` were one feed with two sets of entries.
    scope: &'a str,
    /// The `feed { }` config, for what each format's file is called: the id a
    /// feed writes about itself has to be the file it is served from.
    names: &'a FeedConfig,
    /// Each page's prose, when the site asked its feeds to carry it.
    bodies: &'a Bodies<'a>,
}

impl<'a> Feed<'a> {
    fn new(
        base: &'a BaseUrl,
        title: &'a str,
        description: Option<&'a str>,
        items: &'a [&'a Page],
        scope: &'a str,
        names: &'a FeedConfig,
        bodies: &'a Bodies<'a>,
    ) -> Self {
        Self {
            base,
            title,
            description,
            items,
            scope,
            names,
            bodies,
        }
    }

    /// This page's prose as a feed entry carries it, or `None` for a summary
    /// feed. The one reader of [`Bodies`], so the three formats cannot disagree
    /// about which pages have a body.
    fn body(&self, page: &Page) -> Option<&'a str> {
        self.bodies.get(page)
    }

    /// The feed's own blurb: the configured description, else its title.
    fn blurb(&self) -> &str {
        self.description.unwrap_or(self.title)
    }

    /// Whether this feed has nothing to syndicate.
    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Serialize in the requested format. Item timestamps are rendered up
    /// front: for XML the only fallible step, so the building itself stays
    /// infallible.
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

    /// This feed's language home, the site root for the default language (whose
    /// scope is empty, and which [`Permalink::join`] already reads as "no
    /// segment" rather than as a bare separator).
    fn home(&self) -> String {
        self.base.join(Permalink::join(&[self.scope]))
    }

    /// This feed's own absolute URL, its stable identity.
    fn url(&self, kind: FeedKind) -> String {
        self.names.url(kind, self.base, self.scope)
    }

    fn link(&self, page: &Page) -> String {
        self.base.join(&page.permalink)
    }

    /// The namespace `content:encoded` lives in, declared on `<rss>` whenever a
    /// full feed is written: an undeclared prefix is not well-formed XML, and
    /// several readers drop the whole document over it.
    const CONTENT_NS: (&'static str, &'static str) =
        ("xmlns:content", "http://purl.org/rss/1.0/modules/content/");

    fn rss(&self, xml: &mut Xml, stamps: &[Stamps]) {
        let mut attrs: Vec<(&str, &str)> = vec![("version", "2.0")];
        if self.names.full() {
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
                        // What a reader renders in its list view. Without it
                        // that column is empty and a subscriber sees a wall of
                        // bare titles.
                        if let Some(description) = page.frontmatter.blurb() {
                            xml.leaf("description", description);
                        }
                        // Beside the summary, never instead of it: a list view
                        // wants the short one, and `description` is where every
                        // reader looks for it.
                        if let Some(body) = self.body(page) {
                            xml.leaf("content:encoded", body);
                        }
                        for term in Self::categories(page) {
                            xml.leaf("category", term);
                        }
                        // RSS has no "last changed": `pubDate` is publication,
                        // and an `updated` has nowhere to go in this format.
                        if let Some(published) = &stamp.published {
                            xml.leaf("pubDate", published);
                        }
                    });
                }
            });
        });
    }

    fn atom(&self, xml: &mut Xml, stamps: &[Stamps]) {
        // Items are newest-first, so the first dated one dates the feed.
        let updated = stamps.iter().find_map(Stamps::latest);
        xml.nest("feed", &[("xmlns", "http://www.w3.org/2005/Atom")], |xml| {
            xml.leaf("title", self.title);
            if let Some(description) = self.description {
                xml.leaf("subtitle", description);
            }
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
                    // Atom separates the two moments, so both are said:
                    // `updated` is mandatory on an entry and falls back to the
                    // publication date, `published` is emitted when known.
                    if let Some(updated) = stamp.latest() {
                        xml.leaf("updated", updated);
                    }
                    if let Some(published) = &stamp.published {
                        xml.leaf("published", published);
                    }
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
    /// keyword list in all three standards, with nowhere to say which taxonomy a
    /// term came from.
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

/// One item's two moments, each already in its feed's format.
///
/// Kept apart because the standards do: `published` is when the page went up
/// and orders the feed, `updated` is when it last changed. Feeding one date to
/// both is what left a rewritten post looking untouched to every reader.
struct Stamps {
    published: Option<String>,
    updated: Option<String>,
}

impl Stamps {
    /// The moment that dates this item at all: its `updated` if it has one,
    /// else when it was published. What Atom's `<updated>` requires (it is
    /// mandatory on an entry) and what the feed's own `<updated>` is taken from.
    fn latest(&self) -> Option<&String> {
        self.updated.as_ref().or(self.published.as_ref())
    }
}

/// The JSON Feed 1.1 top-level object: just the required members plus the
/// item list; optional members are omitted, not emitted empty.
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

/// One JSON Feed item. `id` doubles as the canonical `url`, mirroring the
/// `guid`/`link` pairing in the XML formats. Absent members are omitted rather
/// than emitted empty, which the spec asks for and readers rely on.
#[derive(Serialize)]
struct JsonItem<'a> {
    id: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    /// The entry's prose, when the site asked its feeds to carry it. JSON Feed
    /// names it `content_html`, beside `summary` rather than instead of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    content_html: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_published: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_modified: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<&'a str>,
}
