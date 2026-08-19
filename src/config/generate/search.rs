//! `generate { search { } }`: client-side search.

use crate::config::Named;
use crate::config::Value;
use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::{Choice, Flag, Number, Text, Texts};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Client-side search. Enabled by the presence of a `generate { search }`
/// block.
#[derive(Debug, Clone, Hash)]
pub struct SearchConfig {
    pub enabled: bool,
    pub index: SearchIndex,
    pub fields: SearchFields,
    /// Tokens excluded from the index.
    pub stopwords: Vec<String>,
    /// Minimum token length kept in the index.
    pub min_length: usize,
    /// Characters of prose a hit carries as context; `0` carries none, and so
    /// shows no snippet.
    pub snippet: usize,
    pub ui: SearchUi,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            index: SearchIndex::default(),
            fields: SearchFields::default(),
            stopwords: Vec::new(),
            min_length: 2,
            snippet: 240,
            ui: SearchUi::default(),
        }
    }
}

impl SearchConfig {
    /// Whether the build tokenizes the prose itself, rather than shipping it
    /// for the client to index.
    pub fn indexed_here(&self) -> bool {
        matches!(self.index, SearchIndex::Terms)
    }

    /// Whether a page's prose is read at all: to index it, to show it as a
    /// hit's context, or both.
    pub fn carries_prose(&self) -> bool {
        self.fields.body > 0 || self.snippet > 0
    }
}

/// Where a query's postings come from: the payload/latency trade, and nothing
/// else. Both shapes tokenize, rank and snippet identically.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SearchIndex {
    /// Postings built at build time, so the client looks a term up instead of
    /// reading every page's prose. Only each hit's snippet ships.
    #[default]
    Terms,
    /// Every document's prose ships whole and the client indexes it on load:
    /// a larger payload, and the shape any other client library can read.
    Documents,
}

impl Named for SearchIndex {
    const NAMES: &'static [(&'static str, Self)] =
        &[("terms", Self::Terms), ("documents", Self::Documents)];
}

/// What each part of a page is worth to a hit's score; `0` leaves that part out
/// of the index entirely.
#[derive(Debug, Clone, Hash)]
pub struct SearchFields {
    pub title: usize,
    pub tags: usize,
    pub body: usize,
}

impl Default for SearchFields {
    fn default() -> Self {
        Self {
            title: 5,
            tags: 3,
            body: 1,
        }
    }
}

impl SearchFields {
    /// Whether a page's prose is indexed at all, which is what decides if the
    /// build has to extract text from the rendered HTML.
    pub fn indexes_body(&self) -> bool {
        self.body > 0
    }
}

/// The generated command palette. Enabled by the presence of a `ui` block.
#[derive(Debug, Clone, Hash)]
pub struct SearchUi {
    pub enabled: bool,
    /// The key that opens the palette alongside Cmd/Ctrl-K.
    pub hotkey: String,
    pub placeholder: String,
    /// How many hits it shows.
    pub limit: usize,
    /// Whether it injects its own stylesheet.
    pub styles: bool,
}

impl Default for SearchUi {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkey: "/".into(),
            placeholder: "Search".into(),
            limit: 12,
            styles: true,
        }
    }
}

impl Section for SearchConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "index",
            Choice(SearchIndex::names),
            "Where postings come from: `terms` prebuilds them, `documents` ships the prose for the client to index.",
            |c| Value::named(c.index),
            |c, n, t| {
                c.index = n.arg(t, 0)?.one::<SearchIndex>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "fields",
            Nested(SearchFields::rows),
            "What each part of a page is worth to a hit's score. A field at `0` is left out of the index.",
            |c| c.fields.values(),
            |c, n, t| c.fields.fill(n, t),
        ),
        (
            "stopwords",
            Texts,
            "Words to leave out of the index, one word each.",
            |c| c.stopwords.clone().into(),
            |c, n, t| {
                c.stopwords = n.words(t)?;
                Ok(())
            },
        ),
        (
            "minimum",
            Number,
            "The shortest word the index keeps.",
            |c| c.min_length.into(),
            |c, n, t| {
                c.min_length = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "snippet",
            Number,
            "Characters of context a hit shows. `0` shows none, and ships none.",
            |c| c.snippet.into(),
            |c, n, t| {
                c.snippet = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "ui",
            Nested(SearchUi::rows),
            "Ship the generated command palette. Its presence turns it on; `#false` turns it off again.",
            |c| c.ui.values(),
            |c, n, t| c.ui.fill(n, t),
        ),
    ]);
}

impl Section for SearchFields {
    const RULES: Block<Self> = Block(&[
        (
            "title",
            Number,
            "What a match in the page's title is worth.",
            |c| c.title.into(),
            |c, n, t| {
                c.title = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "tags",
            Number,
            "What a match in the page's taxonomy terms is worth.",
            |c| c.tags.into(),
            |c, n, t| {
                c.tags = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "body",
            Number,
            "What a match in the page's prose is worth.",
            |c| c.body.into(),
            |c, n, t| {
                c.body = n.count(t, 0)?;
                Ok(())
            },
        ),
    ]);
}

impl Section for SearchUi {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "hotkey",
            Text,
            "The key that opens the palette when nothing else has focus. Cmd/Ctrl-K is always bound.",
            |c| c.hotkey.clone().into(),
            |c, n, t| {
                c.hotkey = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "placeholder",
            Text,
            "The search box's placeholder text.",
            |c| c.placeholder.clone().into(),
            |c, n, t| {
                c.placeholder = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "limit",
            Number,
            "How many hits the palette shows.",
            |c| c.limit.into(),
            |c, n, t| {
                c.limit = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "styles",
            Flag,
            "Inject the palette's own stylesheet. `#false` leaves it entirely to the site's CSS.",
            |c| c.styles.into(),
            |c, n, t| {
                c.styles = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
