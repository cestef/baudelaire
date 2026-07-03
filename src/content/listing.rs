//! Generated list pages.
//!
//! Taxonomy indexes and paginated collection indexes share one shape: a titled
//! page of links, optionally with prev/next navigation. [`Listing`] captures
//! that shape and lowers it to a synthetic [`Page`] whose body is generated
//! *typst* (never HTML strings), so it compiles like any other page.

use std::fmt::Write;

use crate::codegen::{Content, Str, Value};
use crate::config::Config;
use crate::content::{Frontmatter, Page, PageId};

/// One link row: a labelled link with an optional date (dated collection
/// entries), an optional trailing note (e.g. a taxonomy member count), and the
/// source page's extra frontmatter for the template to draw on.
pub struct Item {
    url: String,
    label: String,
    date: Option<String>,
    note: Option<String>,
    extra: Value,
}

impl Item {
    pub fn new(url: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            label: label.into(),
            date: None,
            note: None,
            extra: Value::dict::<&str>([]),
        }
    }

    /// A row with a trailing `(note)`.
    pub fn noted(url: impl Into<String>, label: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            note: Some(note.into()),
            ..Self::new(url, label)
        }
    }

    /// Attach a display date, shown by dated listings (e.g. a blog index).
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
    /// Owning section id — the synthetic source directory and [`PageId`] prefix.
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
}

impl Listing {
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
        }
    }

    /// A display title from a lowercase section id (`tags` → `Tags`).
    pub fn titlecase(name: &str) -> String {
        let mut chars = name.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    }

    pub fn items(mut self, items: Vec<Item>) -> Self {
        self.items = items;
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
        // Absolute so the world can virtualize it under the project root; the
        // file need not exist (real pages reach absolute paths via canonicalize).
        let base = config
            .content
            .canonicalize()
            .unwrap_or_else(|_| config.content.clone());
        let source = base.join(&self.section).join(format!("{}.typ", self.slug));
        Page {
            id: PageId::new(&self.section, &self.slug),
            source,
            frontmatter: Frontmatter {
                title: Some(self.title.clone()),
                ..Frontmatter::default()
            },
            body: self.body(),
            // A bound template receives this structured data as `page.frontmatter`;
            // without one, the default body renders and this is unused.
            data: self.data().to_string(),
            collection: self.section.clone(),
            output: config.destination(&self.permalink),
            permalink: self.permalink.clone(),
            template: self.template.clone(),
        }
    }

    /// The listing as a typst [`Value`], exposed to a bound template as
    /// `page.frontmatter`: `(title, entries: ((url, label, note), …), nav)`.
    fn data(&self) -> Value {
        let entries = self.items.iter().map(|item| {
            Value::dict([
                ("url", Value::str(&item.url)),
                ("label", Value::str(&item.label)),
                ("date", Value::opt(item.date.clone())),
                ("note", Value::opt(item.note.clone())),
                ("extra", item.extra.clone()),
            ])
        });
        Value::dict([
            ("title", Value::str(&self.title)),
            ("entries", Value::array(entries)),
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
    fn body(&self) -> String {
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
                let _ = write!(out, "#link({})[← Previous] ", Str(prev));
            }
            if let Some(next) = &self.nav.next {
                let _ = write!(out, "#link({})[Next →]", Str(next));
            }
            out.push('\n');
        }
        out
    }
}
