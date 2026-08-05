//! `generate { search { } }`: client-side search indexes.

use crate::config::Named;
use crate::config::dispatch::Kind::{Choice, Flag, Number, Text, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// Client-side search index generation. Empty `formats` disables search.
#[derive(Debug, Clone, Hash)]
pub struct SearchConfig {
    /// Index formats to emit. Empty = disabled.
    pub formats: Vec<SearchFormat>,
    /// Page fields included in each indexed document.
    pub fields: Vec<SearchField>,
    /// The element whose contents are indexed, by tag name. A page without one
    /// is indexed whole.
    pub region: String,
    /// Elements dropped from the indexed region wherever they occur in it, by
    /// tag name: the chrome a layout puts *inside* its content region.
    pub ignore: Vec<String>,
    /// Tokens excluded from the inverted index.
    pub stopwords: Vec<String>,
    /// Minimum token length kept in the inverted index.
    pub min_length: usize,
    /// Also emit the shipped search UI (a Ctrl-K palette) next to each index.
    /// Spelled `ui` in config: the top-level `client { }` block is build-time
    /// constants for client JS, and one name could not mean both.
    pub ui: bool,
}

impl SearchConfig {
    /// Whether the prebuilt inverted index is among the formats emitted. It is
    /// the only one `stopwords` and `minimum` reach, so it is also what decides
    /// whether either of them does anything.
    pub fn inverted(&self) -> bool {
        self.formats.contains(&SearchFormat::Inverted)
    }
}

/// A client-side search index format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchFormat {
    /// A flat document list (`search.json`): pair with any client library
    /// (Fuse.js, MiniSearch, ..), which builds its own index at runtime.
    Json,
    /// A prebuilt inverted index (`search.inverted.json`): server-side tokenized
    /// so the client looks up terms directly instead of scanning every doc.
    Inverted,
}

impl Named for SearchFormat {
    const NAMES: &'static [(&'static str, Self)] =
        &[("json", Self::Json), ("inverted", Self::Inverted)];
}

impl SearchFormat {
    /// The conventional output file name for this format's index.
    pub fn file(self) -> &'static str {
        match self {
            Self::Json => "search.json",
            Self::Inverted => "search.inverted.json",
        }
    }

    /// The file name for this format's generated JavaScript client.
    pub fn client_file(self) -> &'static str {
        match self {
            Self::Json => "search.js",
            Self::Inverted => "search.inverted.js",
        }
    }
}

/// A page field selectable for indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchField {
    Title,
    Body,
    Tags,
}

impl Named for SearchField {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("title", Self::Title),
        ("body", Self::Body),
        ("tags", Self::Tags),
    ];
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            // opt-in: no index until a format is configured
            formats: Vec::new(),
            fields: vec![SearchField::Title, SearchField::Body, SearchField::Tags],
            // The landmark a page's own prose lives in. Indexing the whole
            // document instead puts every page's navigation in every document,
            // which is the fastest way to make a small site's search useless.
            region: "main".into(),
            ignore: Vec::new(),
            stopwords: Vec::new(),
            min_length: 2,
            ui: false,
        }
    }
}

impl Section for SearchConfig {
    const RULES: Block<Self> = Block(&[
        (
            "formats",
            Choice(SearchFormat::names),
            "Which index formats to write, one word each.",
            |c, n, t| {
                c.formats = n.mapped::<SearchFormat>(t)?;
                Ok(())
            },
        ),
        (
            "fields",
            Choice(SearchField::names),
            "Which parts of a page go into the index, one word each.",
            |c, n, t| {
                c.fields = n.mapped::<SearchField>(t)?;
                Ok(())
            },
        ),
        (
            "region",
            Text,
            "The element whose contents are indexed, by tag name. \
             A page without one is indexed whole.",
            |c, n, t| {
                c.region = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "ignore",
            Texts,
            "Elements to leave out of the indexed region, by tag name, one word each.",
            |c, n, t| {
                c.ignore = n.words(t)?;
                Ok(())
            },
        ),
        (
            "stopwords",
            Texts,
            "Words to leave out of the index, one word each.",
            |c, n, t| {
                c.stopwords = n.words(t)?;
                Ok(())
            },
        ),
        (
            "minimum",
            Number,
            "The shortest word the index keeps.",
            |c, n, t| {
                c.min_length = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "ui",
            Flag,
            "Ship the bundled search box as well as the index.",
            |c, n, t| {
                c.ui = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
