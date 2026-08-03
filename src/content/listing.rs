//! Generated list pages.
//!
//! Taxonomy indexes and paginated collection indexes share one shape: a titled
//! page of links, optionally with prev/next navigation. [`Listing`] captures
//! that shape and lowers it to a synthetic [`Page`] whose body is generated
//! *typst* (never HTML strings), so it compiles like any other page.

use std::collections::BTreeMap;
use std::fmt::{self, Write};

use crate::codegen::{Content, Str, Value};
use crate::config::Config;
use crate::content::{Frontmatter, Page, PageId, Strings};

/// Displays a lowercase section id as a title by capitalizing its first letter
/// (`tags` -> `Tags`), the single display rule for generated listing titles,
/// in the `Display`-newtype style of [`crate::ui::Paths`].
pub struct Titlecase<'a>(pub &'a str);

impl fmt::Display for Titlecase<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut chars = self.0.chars();
        match chars.next() {
            Some(first) => write!(f, "{}{}", first.to_uppercase(), chars.as_str()),
            None => Ok(()),
        }
    }
}

/// One link row: a labelled link with an optional date (dated collection
/// entries), an optional trailing note (e.g. a taxonomy member count), and the
/// source page's extra frontmatter for the template to draw on.
///
/// THE page-as-data shape, and not only a listing's: `@baudelaire/pages` and
/// `baudelaire:pages` both serve arrays of [`Item::value`], so a theme writes
/// one card component and renders a collection index, a term page, and a
/// home-page grid with it.
pub struct Item {
    url: String,
    label: String,
    /// The collection the page belongs to, for a consumer picking one out of
    /// the whole catalogue. Empty for a row with no page behind it (a term
    /// index lists terms, not pages).
    collection: String,
    /// The page's language code, empty for the same reason as `collection`.
    lang: String,
    date: Option<String>,
    /// The same date written the way the page's language writes one, so a
    /// listing can show `30 juillet 2026` while `date` stays the ISO-8601 day a
    /// `<time datetime>` wants.
    display: Option<String>,
    note: Option<String>,
    /// The page's taxonomy terms, keyed by taxonomy (e.g. `tags`, `categories`),
    /// exposed as-is so a template picks whichever it wants, never flattened.
    taxonomies: BTreeMap<String, Vec<String>>,
    extra: Value,
}

impl Item {
    pub fn new(url: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            label: label.into(),
            collection: String::new(),
            lang: String::new(),
            date: None,
            display: None,
            note: None,
            taxonomies: BTreeMap::new(),
            extra: Value::dict::<&str>([]),
        }
    }

    /// The listing row for a page, the single page->item mapping, so every
    /// listing (paginated index, taxonomy term) exposes the same data: link,
    /// title, the date in both forms, the page's taxonomies, and its extra
    /// frontmatter (summary, cover image, ..).
    pub fn of(page: &Page, strings: &crate::content::Strings) -> Self {
        let date = page
            .frontmatter
            .date
            .map(|d| crate::content::Iso(d).to_string());
        let display = page
            .frontmatter
            .date
            .map(|d| crate::content::Localized::new(d, strings).to_string());
        let extra = Value::dict(
            page.frontmatter
                .extra
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        Self {
            collection: page.collection.clone(),
            lang: page.lang.clone(),
            ..Self::new(page.permalink.clone(), page.title())
        }
        .dated(date)
        .shown(display)
        .with_taxonomies(page.frontmatter.taxonomies.clone())
        .extra(extra)
    }

    /// This row as a typst [`Value`]: `(url, label, collection, lang, date,
    /// display, note, taxonomies, extra)`.
    ///
    /// The single rendering of a row, so a listing's `entries`, the
    /// `@baudelaire/pages` catalogue and its JavaScript counterpart cannot
    /// drift into three shapes a theme has to learn separately.
    pub fn value(&self) -> Value {
        Value::dict([
            ("url", Value::str(&self.url)),
            ("label", Value::str(&self.label)),
            ("collection", Value::str(&self.collection)),
            ("lang", Value::str(&self.lang)),
            ("date", Value::opt(self.date.clone())),
            ("display", Value::opt(self.display.clone())),
            ("note", Value::opt(self.note.clone())),
            (
                "taxonomies",
                Value::dict(self.taxonomies.iter().map(|(name, terms)| {
                    (name.clone(), Value::array(terms.iter().map(Value::str)))
                })),
            ),
            ("extra", self.extra.clone()),
        ])
    }

    /// Attach the page's taxonomies, exposed to the template as
    /// `entry.taxonomies` (a dict of `taxonomy -> (terms..)`).
    pub fn with_taxonomies(mut self, taxonomies: BTreeMap<String, Vec<String>>) -> Self {
        self.taxonomies = taxonomies;
        self
    }

    /// A row with a trailing `(note)`.
    pub fn noted(
        url: impl Into<String>,
        label: impl Into<String>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            note: Some(note.into()),
            ..Self::new(url, label)
        }
    }

    /// Attach the localized rendering of the same date.
    pub fn shown(mut self, display: Option<String>) -> Self {
        self.display = display;
        self
    }

    /// Attach the ISO date, shown by dated listings (e.g. a blog index).
    pub fn dated(mut self, date: Option<String>) -> Self {
        self.date = date;
        self
    }

    /// Attach the source page's extra frontmatter, exposed to the template as
    /// `entry.extra` so listings can render summaries, cover images, etc.
    pub fn extra(mut self, extra: Value) -> Self {
        self.extra = extra;
        self
    }
}

/// Prev/next links between paginated listings.
#[derive(Default)]
pub struct Nav {
    pub prev: Option<String>,
    pub next: Option<String>,
}

impl Nav {
    fn is_empty(&self) -> bool {
        self.prev.is_none() && self.next.is_none()
    }
}

/// A generated page listing links under a title.
pub struct Listing {
    /// Owning section id, the synthetic source directory and [`PageId`] prefix.
    section: String,
    /// Unique slug within the section (source filename + page id).
    slug: String,
    /// Destination URL path.
    permalink: String,
    title: String,
    items: Vec<Item>,
    nav: Nav,
    /// Optional user template; when set it receives the structured listing data
    /// and controls the whole page, so we aren't opinionated about its look.
    template: Option<String>,
    /// Language this listing belongs to; `None` = the site default.
    lang: Option<String>,
}

impl Listing {
    /// The slug of a section's own index listing (`/tags/`, `/blog/`), as
    /// opposed to a term or a numbered page. Named once: every generator that
    /// emits a section index reads it, and it also names the listing's
    /// synthetic source file.
    pub(crate) const INDEX: &'static str = "index";

    pub fn new(
        section: impl Into<String>,
        slug: impl Into<String>,
        permalink: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            section: section.into(),
            slug: slug.into(),
            permalink: permalink.into(),
            title: title.into(),
            items: Vec::new(),
            nav: Nav::default(),
            template: None,
            lang: None,
        }
    }

    pub fn items(mut self, items: Vec<Item>) -> Self {
        self.items = items;
        self
    }

    /// Assign this listing's language (a non-default one is URL-prefixed).
    pub fn lang(mut self, lang: impl Into<String>) -> Self {
        self.lang = Some(lang.into());
        self
    }

    pub fn nav(mut self, nav: Nav) -> Self {
        self.nav = nav;
        self
    }

    /// Bind a user template that receives the structured listing data.
    pub fn template(mut self, template: Option<String>) -> Self {
        self.template = template;
        self
    }

    /// Lower to a synthetic [`Page`] ready for the compile pipeline.
    pub fn into_page(self, config: &Config) -> Page {
        let lang = self.lang.clone().unwrap_or_else(|| config.lang.clone());
        // Namespace the identity by language so per-language listings get
        // distinct page ids and virtual source paths (the permalink is already
        // localized by the generator). Absolute so the world can virtualize it
        // under the project root; the file need not exist.
        let section = config.scope(&lang, &self.section);
        let base = crate::fs::canonical(&config.paths.content);
        let source = base.join(&section).join(format!("{}.typ", self.slug));
        Page::assemble(
            PageId::new(&section, &self.slug),
            source,
            Frontmatter {
                title: Some(self.title.clone()),
                ..Frontmatter::default()
            },
            self.body(&Strings::new(config, &lang)),
            // A bound template receives this structured data as `page.frontmatter`;
            // without one, the default body renders and this is unused.
            crate::content::Data::Generated {
                dict: crate::codegen::Typst(&self.data()).to_string(),
                lists: self.items.iter().map(|item| item.url.clone()).collect(),
            },
            section,
            &self.permalink,
            self.template.clone(),
            lang,
            config,
        )
    }

    /// The listing as a typst [`Value`], exposed to a bound template as
    /// `page.frontmatter`: `(title, entries: ((url, label, note), ..), nav)`.
    fn data(&self) -> Value {
        Value::dict([
            ("title", Value::str(&self.title)),
            ("entries", Value::array(self.items.iter().map(Item::value))),
            (
                "nav",
                Value::dict([
                    ("prev", Value::opt(self.nav.prev.clone())),
                    ("next", Value::opt(self.nav.next.clone())),
                ]),
            ),
        ])
    }

    /// The generated typst markup: a heading, the link list, then any nav.
    fn body(&self, strings: &Strings) -> String {
        let mut out = format!("#heading(level: 1)[{}]\n\n", Content(&self.title));
        for item in &self.items {
            let _ = write!(out, "- #link({})[{}]", Str(&item.url), Content(&item.label));
            if let Some(note) = &item.note {
                let _ = write!(out, " ({})", Content(note));
            }
            out.push('\n');
        }
        if !self.nav.is_empty() {
            out.push('\n');
            if let Some(prev) = &self.nav.prev {
                let label = Content(strings.get("previous"));
                let _ = write!(out, "#link({})[{label}] ", Str(prev));
            }
            if let Some(next) = &self.nav.next {
                let label = Content(strings.get("next"));
                let _ = write!(out, "#link({})[{label}]", Str(next));
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Nav, Titlecase};

    #[test]
    fn titlecase_capitalizes_first_char_only() {
        assert_eq!(Titlecase("tags").to_string(), "Tags");
        assert_eq!(Titlecase("my notes").to_string(), "My notes");
        assert_eq!(Titlecase("A").to_string(), "A");
        // Empty stays empty; already-capitalized is unchanged.
        assert_eq!(Titlecase("").to_string(), "");
        assert_eq!(Titlecase("Rust").to_string(), "Rust");
    }

    #[test]
    fn nav_is_empty_only_without_links() {
        assert!(Nav::default().is_empty());
        assert!(
            !Nav {
                prev: Some("/a/".into()),
                next: None,
            }
            .is_empty()
        );
        assert!(
            !Nav {
                prev: None,
                next: Some("/b/".into()),
            }
            .is_empty()
        );
    }
}
