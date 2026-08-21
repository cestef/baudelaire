//! `generate { search { } }`: client-side search.

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// Client-side search. Enabled by the presence of a `generate { search }`
/// block.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct SearchConfig {
    pub enabled: bool,

    /// Where postings come from: `terms` prebuilds them, `documents` ships the prose for the client to index.
    #[key(choice(SearchIndex))]
    pub index: SearchIndex,

    /// What each part of a page is worth to a hit's score. A field at `0` is left out of the index.
    #[key(nested(SearchFields))]
    pub fields: SearchFields,

    /// Words to leave out of the index, one word each.
    #[key(texts)]
    pub stopwords: Vec<String>,

    /// The shortest word the index keeps.
    #[key(name = "minimum", count)]
    pub min_length: usize,

    /// Characters of context a hit shows. `0` shows none, and ships none.
    #[key(count)]
    pub snippet: usize,

    /// Ship the generated command palette. Its presence turns it on; `#false` turns it off again.
    #[key(nested(SearchUi))]
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
#[derive(Debug, Clone, Hash, Table)]
pub struct SearchFields {
    /// What a match in the page's title is worth.
    #[key(count)]
    pub title: usize,

    /// What a match in the page's taxonomy terms is worth.
    #[key(count)]
    pub tags: usize,

    /// What a match in the page's prose is worth.
    #[key(count)]
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
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct SearchUi {
    pub enabled: bool,

    /// The key that opens the palette when nothing else has focus. Cmd/Ctrl-K is always bound.
    #[key(text)]
    pub hotkey: String,

    /// The search box's placeholder text.
    #[key(text)]
    pub placeholder: String,

    /// How many hits the palette shows.
    #[key(count)]
    pub limit: usize,

    /// Inject the palette's own stylesheet. `#false` leaves it entirely to the site's CSS.
    #[key(flag)]
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
